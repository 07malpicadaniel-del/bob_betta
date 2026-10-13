use std::sync::atomic::{AtomicU32, AtomicU64};

pub const VECTOR_DIM: usize = 768;
pub type NodeId = u64;


/// Vector con alineación física de 64 bytes para instrucciones SIMD (AVX2/AVX-512)
#[repr(C, align(64))]
#[derive(Clone, Debug)]
pub struct VectorData {
    pub raw: [f32; VECTOR_DIM],
}

impl Default for VectorData {
    fn default() -> Self {
        Self {
            raw: [0.0; VECTOR_DIM],
        }
    }
}

/// Identificadores de las islas independientes definidas por el Tálamo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IslandCategory {
    Code,
    EpisodicMemory,
    CoreRules,
    WorkingMemory,
}

/// Metadatos mutables de bajo nivel controlados por operaciones atómicas
pub struct NodeMetadata {
    pub id: NodeId,
    pub created_at: u64,
    pub last_access: AtomicU64,
    pub topological_weight: AtomicU32,
    pub category: IslandCategory,
}
