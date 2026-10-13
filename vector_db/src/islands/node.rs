use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use parking_lot::RwLock;
use crate::common::types::{NodeId, VectorData};

pub const MAX_HNSW_LAYERS: usize = 16;

pub struct IslandNode {
    pub id: NodeId,
    pub vector: VectorData,
    /// Preasignación fija de capas para permitir promoción concurrente sin reasignar memoria
    pub neighbors: Vec<RwLock<Vec<NodeId>>>,
    pub active_layer: AtomicUsize,
    pub last_access: AtomicU64,
    pub topological_weight: AtomicU32,
}

impl IslandNode {
    pub fn new(id: NodeId, vector: VectorData, initial_layer: usize, timestamp: u64, initial_weight: u32) -> Self {
        let mut neighbors = Vec::with_capacity(MAX_HNSW_LAYERS);
        for _ in 0..MAX_HNSW_LAYERS {
            neighbors.push(RwLock::new(Vec::new()));
        }

        Self {
            id,
            vector,
            neighbors,
            active_layer: AtomicUsize::new(initial_layer.min(MAX_HNSW_LAYERS - 1)),
            last_access: AtomicU64::new(timestamp),
            topological_weight: AtomicU32::new(initial_weight),
        }
    }

    #[inline(always)]
    pub fn touch(&self, current_time: u64) {
        self.last_access.store(current_time, Ordering::Relaxed);
        self.topological_weight.fetch_add(1, Ordering::Relaxed);
    }
}
