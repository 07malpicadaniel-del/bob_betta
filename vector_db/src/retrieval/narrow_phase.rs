use super::broad_phase::ScoredCandidate;
use std::cmp::Ordering as CmpOrdering;
use std::sync::atomic::Ordering;

pub struct NarrowPhaseFilter;

impl NarrowPhaseFilter {
    /// Reclasifica candidatos combinando proximidad angular y peso topológico.
    /// Mapea la similitud de coseno de [-1.0, 1.0] a [0.0, 1.0] para evitar inversiones de signo
    /// y añade el estímulo logarítmico del peso acumulado (neuroplasticidad).
    pub fn refine_top_k(
        mut candidates: Vec<ScoredCandidate>,
        top_k: usize,
        alpha: f32,
        current_time: u64,
    ) -> Vec<ScoredCandidate> {
        for c in candidates.iter_mut() {
            let weight = c.node.topological_weight.load(Ordering::Relaxed) as f32;
            let norm_sim = (c.cosine_sim + 1.0) * 0.5;
            c.final_score = norm_sim + (alpha * (1.0 + weight).ln());
        }

        candidates.sort_by(|a, b| {
            b.final_score.partial_cmp(&a.final_score).unwrap_or(CmpOrdering::Equal)
        });

        // Registrar acceso (neuroplasticidad) en los nodos seleccionados
        for c in candidates.iter().take(top_k) {
            c.node.touch(current_time);
        }

        candidates.truncate(top_k);
        candidates
    }
}
