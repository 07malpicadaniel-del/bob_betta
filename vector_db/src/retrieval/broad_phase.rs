use std::collections::{BinaryHeap, HashSet};
use std::sync::Arc;
use crate::common::simd::dot_product;
use crate::common::types::{NodeId, VectorData};
use crate::islands::island::VectorIsland;
use crate::islands::node::IslandNode;

#[derive(Clone)]
pub struct ScoredCandidate {
    pub id: NodeId,
    pub cosine_sim: f32,
    pub final_score: f32,
    pub node: Arc<IslandNode>,
}

impl PartialEq for ScoredCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ScoredCandidate {}

impl Ord for ScoredCandidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cosine_sim.partial_cmp(&other.cosine_sim).unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl PartialOrd for ScoredCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub struct BroadPhaseRetrieval;

impl BroadPhaseRetrieval {
    /// Recupera los N vectores más cercanos navegando la capa base del HNSW con SIMD
    pub fn search_candidates(
        island: &VectorIsland,
        query: &VectorData,
        limit: usize,
    ) -> Vec<ScoredCandidate> {
        let ep = match *island.entry_point.read() {
            Some(id) => id,
            None => return Vec::new(),
        };

        let max_layer = island.max_layer.load(std::sync::atomic::Ordering::Relaxed);
        let mut curr_entry = ep;

        // Descenso rápido en capas altas
        for l in (1..=max_layer).rev() {
            curr_entry = island.graph.search_layer_greedy(query, curr_entry, l, &island.nodes);
        }

        // Búsqueda en capa base 0
        let mut visited = HashSet::new();
        let mut queue = BinaryHeap::new();
        let mut candidates = Vec::with_capacity(limit);

        if let Some(start_node) = island.nodes.get(&curr_entry) {
            let sim = dot_product(query, &start_node.vector);
            visited.insert(curr_entry);
            queue.push(ScoredCandidate {
                id: curr_entry,
                cosine_sim: sim,
                final_score: sim,
                node: start_node.clone(),
            });
        }

        while let Some(candidate) = queue.pop() {
            candidates.push(candidate.clone());
            if candidates.len() >= limit {
                break;
            }

            let neighbors = candidate.node.neighbors[0].read().clone();
            for nb_id in neighbors {
                if visited.insert(nb_id) {
                    if let Some(nb_node) = island.nodes.get(&nb_id) {
                        let sim = dot_product(query, &nb_node.vector);
                        queue.push(ScoredCandidate {
                            id: nb_id,
                            cosine_sim: sim,
                            final_score: sim,
                            node: nb_node.clone(),
                        });
                    }
                }
            }
        }

        candidates
    }
}
