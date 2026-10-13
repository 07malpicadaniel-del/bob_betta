use super::broad_phase::ScoredCandidate;

pub struct ContextAssembler;

impl ContextAssembler {
    /// Formatea los recuerdos recuperados en un bloque inyectable en el prompt de Bob,
    /// mostrando la similitud de coseno pura, el score topológico compuesto y el ID.
    pub fn build_rag_context(records: &[(ScoredCandidate, String)]) -> String {
        if records.is_empty() {
            return String::new();
        }

        let mut context = String::from("### MEMORIA RECUPERADA (HIPOCAMPO):\n");
        for (i, (candidate, text)) in records.iter().enumerate() {
            context.push_str(&format!(
                "[{}] (Coseno: {:.4}, Score Topológico: {:.4}, Nodo ID: {}) {}\n",
                i + 1,
                candidate.cosine_sim,
                candidate.final_score,
                candidate.id,
                text.trim()
            ));
        }
        context.push_str("---\n");
        context
    }
}
