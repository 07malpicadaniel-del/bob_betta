use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::common::types::{IslandCategory, NodeId, VectorData, VECTOR_DIM};
use crate::ingestion::normalizer::VectorNormalizer;

pub type Timestamp = u64;

#[inline(always)]
fn dot_product_768(a: &VectorData, b: &VectorData) -> f32 {
    let mut sum = 0.0f32;
    for i in 0..VECTOR_DIM {
        sum += a.raw[i] * b.raw[i];
    }
    sum
}

/// Tipos de aristas causales dirigidas en el Plano 2 del Hipocampo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CausalRelation {
    DependeDe,
    FallaCuando,
    Optimiza,
    Implementa,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalEdge {
    pub target_id: NodeId,
    pub relation: CausalRelation,
    pub target_summary: String,
    pub weight: f32,
    pub timestamp: Timestamp,
}

/// Estructura serializable en binario puro (bincode) para persistir el Plano 2 en disco
#[derive(Serialize, Deserialize, Default)]
struct CausalSnapshot {
    edges: Vec<(NodeId, Vec<CausalEdge>)>,
    validations: Vec<(NodeId, u32)>,
}

/// Plano 2: Hipergrafo Causal Dirigido + Potenciación a Largo Plazo (LTP) Lock-Free
pub struct CausalHypergraph {
    edges: DashMap<NodeId, Vec<CausalEdge>>,
    validations: DashMap<NodeId, u32>,
    base_lambda: f32,
    alpha: f32,
    storage_path: Option<String>,
}

impl CausalHypergraph {
    pub fn new(base_lambda: f32, alpha: f32, storage_path: Option<&str>) -> Self {
        let graph = Self {
            edges: DashMap::new(),
            validations: DashMap::new(),
            base_lambda,
            alpha,
            storage_path: storage_path.map(|s| s.to_string()),
        };
        let _ = graph.load_from_disk();
        graph
    }

    /// Registra una arista causal dirigida (source -> target) y persiste en disco
    pub fn link_nodes(
        &self,
        source_id: NodeId,
        target_id: NodeId,
        relation: CausalRelation,
        target_summary: String,
        weight: f32,
        now: Timestamp,
    ) {
        if source_id == target_id {
            return;
        }
        let edge = CausalEdge {
            target_id,
            relation,
            target_summary,
            weight,
            timestamp: now,
        };
        let mut entry = self.edges.entry(source_id).or_default();
        if !entry.iter().any(|e| e.target_id == target_id && e.relation == relation) {
            entry.push(edge);
        }
        drop(entry);
        let _ = self.save_to_disk();
    }

    /// Incrementa las validaciones empíricas de un nodo (LTP: reduce su lambda hacia 0)
    pub fn record_validation(&self, node_id: NodeId) -> u32 {
        let mut count = self.validations.entry(node_id).or_insert(0);
        *count += 1;
        let current = *count;
        drop(count);
        let _ = self.save_to_disk();
        current
    }

    /// Ecuación de Potenciación a Largo Plazo (Fase 3.2):
    /// \lambda_{nuevo} = \frac{\lambda_{inicial}}{1 + \alpha \cdot \ln(1 + N_{validaciones})}
    #[inline]
    pub fn effective_lambda(&self, node_id: NodeId, is_core_axiom: bool) -> f32 {
        if is_core_axiom {
            return 0.0;
        }
        let n_val = self.validations.get(&node_id).map(|v| *v as f32).unwrap_or(0.0);
        self.base_lambda / (1.0 + self.alpha * (1.0 + n_val).ln())
    }

    /// Propagación de activación O(E) sobre múltiples Nodos Semilla recuperados por HNSW
    pub fn propagate_from_seeds(
        &self,
        seed_ids: &[NodeId],
        now: Timestamp,
    ) -> Vec<(NodeId, CausalRelation, NodeId, String, f32)> {
        let mut activated = Vec::new();
        for &seed_id in seed_ids {
            if let Some(edge_list) = self.edges.get(&seed_id) {
                let lambda = self.effective_lambda(seed_id, false);
                for edge in edge_list.iter() {
                    let dt = now.saturating_sub(edge.timestamp) as f32;
                    let energy = edge.weight * (-lambda * dt).exp();
                    if energy > 0.15 {
                        activated.push((
                            seed_id,
                            edge.relation,
                            edge.target_id,
                            edge.target_summary.clone(),
                            energy,
                        ));
                    }
                }
            }
        }
        activated
    }

    /// Extrae dinámicamente los NodeId presentes en el bloque de texto devuelto por HippocampusEngine::recall
    pub fn extract_seed_ids(recall_context: &str) -> Vec<NodeId> {
        let mut ids = Vec::new();
        let marker = "Nodo ID: ";
        let mut search_slice = recall_context;

        while let Some(pos) = search_slice.find(marker) {
            let after = &search_slice[pos + marker.len()..];
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(id) = digits.parse::<NodeId>() {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            search_slice = after;
        }
        ids
    }

    pub fn total_edges(&self) -> usize {
        self.edges.iter().map(|e| e.value().len()).sum()
    }

    pub fn save_to_disk(&self) -> Result<(), String> {
        let path_str = match &self.storage_path {
            Some(p) => p,
            None => return Ok(()),
        };

        let snapshot = CausalSnapshot {
            edges: self.edges.iter().map(|kv| (*kv.key(), kv.value().clone())).collect(),
            validations: self.validations.iter().map(|kv| (*kv.key(), *kv.value())).collect(),
        };

        let file = File::create(path_str).map_err(|e| e.to_string())?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, &snapshot).map_err(|e| e.to_string())
    }

    pub fn load_from_disk(&self) -> Result<usize, String> {
        let path_str = match &self.storage_path {
            Some(p) => p,
            None => return Ok(0),
        };

        if !Path::new(path_str).exists() {
            return Ok(0);
        }

        let file = File::open(path_str).map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);
        let snapshot: CausalSnapshot = bincode::deserialize_from(reader).map_err(|e| e.to_string())?;

        let mut count = 0;
        for (k, v) in snapshot.edges {
            count += v.len();
            self.edges.insert(k, v);
        }
        for (k, v) in snapshot.validations {
            self.validations.insert(k, v);
        }
        Ok(count)
    }
}

/// Tálamo Vectorial en <1 µs basado en 4 Centroides en Memoria (Fase 3.5)
pub struct VectorThalamus {
    centroids: RwLock<[(IslandCategory, VectorData); 4]>,
}

impl VectorThalamus {
    pub fn new(
        code_proto: VectorData,
        rules_proto: VectorData,
        working_proto: VectorData,
        episodic_proto: VectorData,
    ) -> Self {
        Self {
            centroids: RwLock::new([
                (IslandCategory::Code, code_proto),
                (IslandCategory::CoreRules, rules_proto),
                (IslandCategory::WorkingMemory, working_proto),
                (IslandCategory::EpisodicMemory, episodic_proto),
            ]),
        }
    }

    #[inline]
    pub fn route_vector(&self, query_vec: &VectorData) -> (IslandCategory, f32) {
        let guards = self.centroids.read();
        let mut best_cat = IslandCategory::WorkingMemory;
        let mut best_sim = f32::MIN;

        for (cat, centroid) in guards.iter() {
            let sim = dot_product_768(query_vec, centroid);
            if sim > best_sim {
                best_sim = sim;
                best_cat = *cat;
            }
        }

        (best_cat, best_sim)
    }

    pub fn reinforce_centroid(&self, category: IslandCategory, sample: &VectorData, lr: f32) {
        let mut guards = self.centroids.write();
        for (cat, centroid) in guards.iter_mut() {
            if *cat == category {
                for i in 0..VECTOR_DIM {
                    centroid.raw[i] = (1.0 - lr) * centroid.raw[i] + lr * sample.raw[i];
                }
                VectorNormalizer::normalize_l2(centroid);
                break;
            }
        }
    }
}

impl CausalHypergraph {
    /// Genera un vector unitario aleatorio en la hiperesfera de 768 dimensiones para muestreo dinámico HNSW
    pub fn random_hypersphere_probe() -> VectorData {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut vec = VectorData::default();
        for i in 0..VECTOR_DIM {
            vec.raw[i] = rng.gen_range(-1.0f32..1.0f32);
        }
        VectorNormalizer::normalize_l2(&mut vec);
        vec
    }

    /// Extrae pares (NodeId, Resumen) desde la salida de HippocampusEngine::recall
    pub fn extract_nodes_with_text(recall_context: &str) -> Vec<(NodeId, String)> {
        let mut results = Vec::new();
        let marker = "Nodo ID: ";

        for line in recall_context.lines() {
            if let Some(pos) = line.find(marker) {
                let after = &line[pos + marker.len()..];
                let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(id) = digits.parse::<NodeId>() {
                    if let Some(close_paren) = after.find(')') {
                        let text = after[close_paren + 1..].trim().to_string();
                        if text.len() > 45 && !text.contains("/tink") && !results.iter().any(|(existing_id, _)| *existing_id == id) {
                            results.push((id, text));
                        }
                    }
                }
            }
        }
        results
    }
}
