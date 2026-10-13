use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;
use crate::common::types::{IslandCategory, NodeId, NodeMetadata, VectorData};
use super::normalizer::VectorNormalizer;

pub struct MemoryPayload {
    pub id: NodeId,
    pub text_content: String,
    pub vector: VectorData,
    pub category: IslandCategory,
    pub initial_weight: u32,
}

impl MemoryPayload {
    pub fn new(
        id: NodeId,
        text_content: String,
        mut raw_vector: VectorData,
        category: IslandCategory,
        initial_weight: u32,
        current_time: u64,
    ) -> (Self, Arc<NodeMetadata>) {
        // Garantizar norma unitaria previa a la indexación
        VectorNormalizer::normalize_l2(&mut raw_vector);

        let metadata = Arc::new(NodeMetadata {
            id,
            created_at: current_time,
            last_access: AtomicU64::new(current_time),
            topological_weight: AtomicU32::new(initial_weight),
            category,
        });

        let payload = Self {
            id,
            text_content,
            vector: raw_vector,
            category,
            initial_weight,
        };

        (payload, metadata)
    }
}
