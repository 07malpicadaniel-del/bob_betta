use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crate::common::types::IslandCategory;
use crate::islands::island::VectorIsland;
use super::topology::TopologicalDecay;

pub struct DecayWorker {
    pub ttl_seconds: u64,
    pub lambda: f32,
    pub promotion_threshold: u32,
    pub is_running: Arc<AtomicBool>,
}

impl DecayWorker {
    pub fn new(ttl_seconds: u64, lambda: f32, promotion_threshold: u32) -> Self {
        Self {
            ttl_seconds,
            lambda,
            promotion_threshold,
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Ciclo de mantenimiento completo: poda de working memory y promoción de conceptos clave
    pub fn run_maintenance_cycle(&self, islands: &HashMap<IslandCategory, VectorIsland>, current_time: u64) -> (usize, usize) {
        let mut pruned = 0;
        let mut promoted = 0;

        for (category, island) in islands.iter() {
            // 1. Olvido Activo: Poda de memoria de trabajo
            if *category == IslandCategory::WorkingMemory {
                let mut to_evict = Vec::new();
                for entry in island.nodes.iter() {
                    let node = entry.value();
                    if TopologicalDecay::is_stale(node, current_time, self.ttl_seconds, 3) {
                        to_evict.push(*entry.key());
                    }
                }

                for id in to_evict {
                    if let Some((_, dead_node)) = island.nodes.remove(&id) {
                        let active_l = dead_node.active_layer.load(Ordering::Relaxed);
                        for layer in 0..=active_l {
                            let neighbors = dead_node.neighbors[layer].read().clone();
                            for neighbor_id in neighbors {
                                if let Some(neighbor_node) = island.nodes.get(&neighbor_id) {
                                    neighbor_node.neighbors[layer].write().retain(|&x| x != id);
                                }
                            }
                        }
                        pruned += 1;
                    }
                }
            }

            // 2. Curvatura Dinámica: Promoción de nodos recurrentes
            let mut to_promote = Vec::new();
            for entry in island.nodes.iter() {
                let node = entry.value();
                if TopologicalDecay::should_promote(node, self.promotion_threshold) {
                    to_promote.push(*entry.key());
                }
            }

            for id in to_promote {
                if island.promote_node(id) {
                    promoted += 1;
                }
            }
        }

        (pruned, promoted)
    }
}
