pub mod common;
pub mod ingestion;
pub mod thalamus;
pub mod islands;
pub mod decay;
pub mod retrieval;
pub mod storage;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use dashmap::DashMap;
use parking_lot::Mutex;

use common::types::{IslandCategory, NodeId, VectorData};
use ingestion::normalizer::VectorNormalizer;
use thalamus::router::ThalamusRouter;
use islands::island::VectorIsland;
use decay::worker::DecayWorker;
use retrieval::broad_phase::BroadPhaseRetrieval;
use retrieval::narrow_phase::NarrowPhaseFilter;
use retrieval::assembler::ContextAssembler;
use storage::wal::{WalRecord, WriteAheadLog};

pub struct HippocampusEngine {
    pub islands: Arc<HashMap<IslandCategory, VectorIsland>>,
    pub memory_store: DashMap<NodeId, String>,
    pub decay_worker: DecayWorker,
    pub wal: Option<Mutex<WriteAheadLog>>,
    pub wal_path: Option<PathBuf>,
}

impl HippocampusEngine {
    pub fn new(
        m: usize,
        ef_construction: usize,
        ttl_seconds: u64,
        lambda: f32,
        promotion_threshold: u32,
        wal_path: Option<&str>,
    ) -> Self {
        let mut islands = HashMap::new();
        islands.insert(IslandCategory::Code, VectorIsland::new(IslandCategory::Code, m, ef_construction));
        islands.insert(IslandCategory::EpisodicMemory, VectorIsland::new(IslandCategory::EpisodicMemory, m, ef_construction));
        islands.insert(IslandCategory::CoreRules, VectorIsland::new(IslandCategory::CoreRules, m, ef_construction));
        islands.insert(IslandCategory::WorkingMemory, VectorIsland::new(IslandCategory::WorkingMemory, m, ef_construction));

        let (wal, path_buf) = match wal_path {
            Some(path) => {
                let log = WriteAheadLog::open(path).expect("No se pudo inicializar WAL");
                (Some(Mutex::new(log)), Some(PathBuf::from(path)))
            }
            None => (None, None),
        };

        Self {
            islands: Arc::new(islands),
            memory_store: DashMap::new(),
            decay_worker: DecayWorker::new(ttl_seconds, lambda, promotion_threshold),
            wal,
            wal_path: path_buf,
        }
    }

    /// Reconstruye el estado del hipocampo a partir del WAL
    pub fn recover_from_wal(&self) -> usize {
        let path = match &self.wal_path {
            Some(p) => p,
            None => return 0,
        };

        let recovered = match WriteAheadLog::replay(path) {
            Ok(records) => records,
            Err(_) => return 0,
        };

        let count = recovered.len();
        for record in recovered {
            let island = self.islands.get(&record.category).expect("Isla ausente");
            island.insert(record.vector, 0, 1);
            self.memory_store.insert(record.id, record.text);
        }

        count
    }

    pub fn remember(
        &self,
        text: String,
        mut raw_vector: VectorData,
        explicit_category: Option<IslandCategory>,
        initial_weight: u32,
        current_time: u64,
    ) -> (NodeId, IslandCategory) {
        VectorNormalizer::normalize_l2(&mut raw_vector);
        let category = explicit_category.unwrap_or_else(|| ThalamusRouter::route_intent(&text));
        let island = self.islands.get(&category).expect("Isla no inicializada");

        let node_id = island.insert(raw_vector.clone(), current_time, initial_weight);
        self.memory_store.insert(node_id, text.clone());

        // Registro secuencial en disco
        if let Some(wal_mutex) = &self.wal {
            let mut wal_guard = wal_mutex.lock();
            let record = WalRecord {
                id: node_id,
                category,
                vector: &raw_vector,
                text: &text,
            };
            let _ = wal_guard.append(&record);
        }

        (node_id, category)
    }

    pub fn recall(
        &self,
        query_prompt: &str,
        mut query_vector: VectorData,
        top_k: usize,
        current_time: u64,
    ) -> String {
        VectorNormalizer::normalize_l2(&mut query_vector);
        let category = ThalamusRouter::route_intent(query_prompt);
        let island = self.islands.get(&category).expect("Isla no inicializada");

        let candidates = BroadPhaseRetrieval::search_candidates(island, &query_vector, 50);
        let refined = NarrowPhaseFilter::refine_top_k(candidates, top_k, 0.25, current_time);

        let mut results = Vec::new();
        for candidate in refined {
            if let Some(text) = self.memory_store.get(&candidate.id) {
                results.push((candidate, text.clone()));
            }
        }

        ContextAssembler::build_rag_context(&results)
    }

    pub fn start_background_maintenance(self: &Arc<Self>, interval: Duration) -> thread::JoinHandle<()> {
        let is_running = self.decay_worker.is_running.clone();
        is_running.store(true, Ordering::SeqCst);

        let engine = Arc::clone(self);
        thread::spawn(move || {
            while is_running.load(Ordering::Relaxed) {
                thread::sleep(interval);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let _ = engine.decay_worker.run_maintenance_cycle(&engine.islands, now);
            }
        })
    }

    pub fn stop_background_maintenance(&self) {
        self.decay_worker.is_running.store(false, Ordering::SeqCst);
    }
}
pub mod causal_plane;
