use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use dashmap::DashMap;
use parking_lot::RwLock;

use crate::common::types::{IslandCategory, NodeId, VectorData};
use super::hnsw::HnswGraph;
use super::node::{IslandNode, MAX_HNSW_LAYERS};

pub struct VectorIsland {
    pub category: IslandCategory,
    pub nodes: DashMap<NodeId, Arc<IslandNode>>,
    pub graph: HnswGraph,
    pub entry_point: RwLock<Option<NodeId>>,
    pub max_layer: AtomicUsize,
    pub node_counter: AtomicU64,
}

impl VectorIsland {
    pub fn new(category: IslandCategory, m: usize, ef_construction: usize) -> Self {
        Self {
            category,
            nodes: DashMap::new(),
            graph: HnswGraph::new(m, ef_construction),
            entry_point: RwLock::new(None),
            max_layer: AtomicUsize::new(0),
            node_counter: AtomicU64::new(0),
        }
    }

    pub fn insert(&self, vector: VectorData, timestamp: u64, initial_weight: u32) -> NodeId {
        let id = self.node_counter.fetch_add(1, Ordering::SeqCst);
        let raw_layer = self.graph.random_layer();
        let node_layer = raw_layer.min(MAX_HNSW_LAYERS - 1);
        let new_node = Arc::new(IslandNode::new(id, vector.clone(), node_layer, timestamp, initial_weight));
        
        self.nodes.insert(id, new_node.clone());

        let mut ep_guard = self.entry_point.write();
        let current_ep = *ep_guard;

        match current_ep {
            None => {
                *ep_guard = Some(id);
                self.max_layer.store(node_layer, Ordering::SeqCst);
            }
            Some(mut ep) => {
                let current_max_l = self.max_layer.load(Ordering::SeqCst);

                if node_layer < current_max_l {
                    for l in ((node_layer + 1)..=current_max_l).rev() {
                        ep = self.graph.search_layer_greedy(&vector, ep, l, &self.nodes);
                    }
                }

                let min_l = node_layer.min(current_max_l);
                for l in (0..=min_l).rev() {
                    let nearest = self.graph.search_layer_greedy(&vector, ep, l, &self.nodes);
                    new_node.neighbors[l].write().push(nearest);
                    
                    if let Some(target) = self.nodes.get(&nearest) {
                        target.neighbors[l].write().push(id);
                    }
                    ep = nearest;
                }

                if node_layer > current_max_l {
                    *ep_guard = Some(id);
                    self.max_layer.store(node_layer, Ordering::SeqCst);
                }
            }
        }

        id
    }

    /// Curvatura Dinámica: Promueve un nodo hacia la capa L + 1 conectándolo bidireccionalmente
    pub fn promote_node(&self, id: NodeId) -> bool {
        let node = match self.nodes.get(&id) {
            Some(n) => n.clone(),
            None => return false,
        };

        let current_layer = node.active_layer.load(Ordering::SeqCst);
        if current_layer >= MAX_HNSW_LAYERS - 1 {
            return false;
        }

        let target_layer = current_layer + 1;
        let ep = match *self.entry_point.read() {
            Some(entry) => entry,
            None => return false,
        };

        // Enlazar al vecino más cercano en la nueva capa
        let nearest = self.graph.search_layer_greedy(&node.vector, ep, target_layer, &self.nodes);
        if nearest != id {
            node.neighbors[target_layer].write().push(nearest);
            if let Some(nb) = self.nodes.get(&nearest) {
                nb.neighbors[target_layer].write().push(id);
            }
        }

        node.active_layer.store(target_layer, Ordering::SeqCst);

        // Actualizar la cúspide de la pirámide si sobrepasa la capa máxima
        let mut current_max = self.max_layer.load(Ordering::SeqCst);
        while target_layer > current_max {
            match self.max_layer.compare_exchange_weak(
                current_max,
                target_layer,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let mut ep_guard = self.entry_point.write();
                    *ep_guard = Some(id);
                    break;
                }
                Err(actual) => current_max = actual,
            }
        }

        true
    }
}
