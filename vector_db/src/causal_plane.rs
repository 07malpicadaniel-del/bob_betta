use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::common::types::{IslandCategory, NodeId, VectorData, VECTOR_DIM};
use crate::ingestion::normalizer::VectorNormalizer;

pub type Timestamp = u64;

/// Producto punto SIMD sobre vectores de 768 dimensiones alineados a 64 bytes
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

/// Plano 2: Hipergrafo Causal Dirigido + Potenciación a Largo Plazo (LTP) Lock-Free
pub struct CausalHypergraph {
    edges: DashMap<NodeId, Vec<CausalEdge>>,
    validations: DashMap<NodeId, u32>,
    base_lambda: f32,
    alpha: f32,
}

impl CausalHypergraph {
    pub fn new(base_lambda: f32, alpha: f32) -> Self {
        Self {
            edges: DashMap::new(),
            validations: DashMap::new(),
            base_lambda,
            alpha,
        }
    }

    /// Registra una arista causal dirigida (source -> target)
    pub fn link_nodes(
        &self,
        source_id: NodeId,
        target_id: NodeId,
        relation: CausalRelation,
        target_summary: String,
        weight: f32,
        now: Timestamp,
    ) {
        let edge = CausalEdge {
            target_id,
            relation,
            target_summary,
            weight,
            timestamp: now,
        };
        self.edges.entry(source_id).or_default().push(edge);
    }

    /// Incrementa las validaciones empíricas de un nodo (reduce su lambda hacia 0)
    pub fn record_validation(&self, node_id: NodeId) -> u32 {
        let mut count = self.validations.entry(node_id).or_insert(0);
        *count += 1;
        *count
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

    /// Propagación de activación O(E) sobre el Plano 2 a partir de un Nodo Semilla
    pub fn propagate_causal_activation(&self, seed_id: NodeId, now: Timestamp) -> Vec<(CausalRelation, NodeId, String, f32)> {
        let mut activated = Vec::new();
        if let Some(edge_list) = self.edges.get(&seed_id) {
            let lambda = self.effective_lambda(seed_id, false);
            for edge in edge_list.iter() {
                let dt = now.saturating_sub(edge.timestamp) as f32;
                let energy = edge.weight * (-lambda * dt).exp();
                if energy > 0.15 {
                    activated.push((
                        edge.relation,
                        edge.target_id,
                        edge.target_summary.clone(),
                        energy,
                    ));
                }
            }
        }
        activated
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

    /// Enruta el vector de entrada calculando 4 productos punto SIMD en <1 µs
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

    /// Refuerzo dinámico del centroide por media móvil exponencial (EMA)
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
