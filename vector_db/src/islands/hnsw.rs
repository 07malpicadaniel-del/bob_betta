use std::cmp::Ordering as CmpOrdering;
use std::sync::Arc;
use dashmap::DashMap;
use rand::Rng;

use crate::common::simd::dot_product;
use crate::common::types::{NodeId, VectorData};
use super::node::IslandNode;

#[derive(Clone, Copy)]
pub struct Candidate {
    pub id: NodeId,
    pub distance: f32, // En vectores normalizados: mayor dot_product = menor distancia angular
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Candidate {}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // Max-Heap por distancia angular invertida
        other.distance.partial_cmp(&self.distance).unwrap_or(CmpOrdering::Equal)
    }
}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

pub struct HnswGraph {
    pub m: usize,              // Conexiones máximas por capa
    pub m0: usize,             // Conexiones máximas en capa 0
    pub ef_construction: usize,// Amplitud de búsqueda durante inserción
    pub ml: f64,               // Factor de decaimiento de capas
}

impl HnswGraph {
    pub fn new(m: usize, ef_construction: usize) -> Self {
        let m0 = 2 * m;
        let ml = 1.0 / (m as f64).ln();
        Self {
            m,
            m0,
            ef_construction,
            ml,
        }
    }

    pub fn random_layer(&self) -> usize {
        let mut rng = rand::thread_rng();
        let r: f64 = rng.gen();
        (-r.ln() * self.ml).floor() as usize
    }

    /// Búsqueda codiciosa (Greedy) para descender niveles de la pirámide HNSW
    pub fn search_layer_greedy(
        &self,
        query: &VectorData,
        mut current_id: NodeId,
        layer: usize,
        nodes: &DashMap<NodeId, Arc<IslandNode>>,
    ) -> NodeId {
        let mut current_node = match nodes.get(&current_id) {
            Some(n) => n.clone(),
            None => return current_id,
        };

        let mut current_sim = dot_product(query, &current_node.vector);

        loop {
            let mut changed = false;
            let neighbors = current_node.neighbors[layer].read().clone();

            for neighbor_id in neighbors {
                if let Some(neighbor_node) = nodes.get(&neighbor_id) {
                    let sim = dot_product(query, &neighbor_node.vector);
                    if sim > current_sim {
                        current_sim = sim;
                        current_id = neighbor_id;
                        current_node = neighbor_node.clone();
                        changed = true;
                        break;
                    }
                }
            }

            if !changed {
                break;
            }
        }

        current_id
    }
}
