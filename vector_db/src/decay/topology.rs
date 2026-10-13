use std::sync::atomic::Ordering;
use crate::islands::node::{IslandNode, MAX_HNSW_LAYERS};

pub struct TopologicalDecay;

impl TopologicalDecay {
    #[inline]
    pub fn compute_dynamic_score(node: &IslandNode, current_time: u64, lambda: f32) -> f32 {
        let last_access = node.last_access.load(Ordering::Relaxed);
        let weight = node.topological_weight.load(Ordering::Relaxed) as f32;
        let delta_seconds = current_time.saturating_sub(last_access) as f32;
        weight * (-lambda * delta_seconds).exp()
    }

    #[inline]
    pub fn is_stale(node: &IslandNode, current_time: u64, ttl_seconds: u64, min_weight: u32) -> bool {
        let last_access = node.last_access.load(Ordering::Relaxed);
        let weight = node.topological_weight.load(Ordering::Relaxed);
        let elapsed = current_time.saturating_sub(last_access);
        elapsed > ttl_seconds && weight < min_weight
    }

    /// Curvatura Dinámica: un nodo se promueve si sus accesos superan el umbral para su capa actual
    #[inline]
    pub fn should_promote(node: &IslandNode, threshold_per_layer: u32) -> bool {
        let current_layer = node.active_layer.load(Ordering::Relaxed);
        if current_layer >= MAX_HNSW_LAYERS - 1 {
            return false;
        }
        let weight = node.topological_weight.load(Ordering::Relaxed);
        weight >= (current_layer as u32 + 1) * threshold_per_layer
    }
}
