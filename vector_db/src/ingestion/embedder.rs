use serde::{Deserialize, Serialize};
use crate::common::types::{VectorData, VECTOR_DIM};
use super::normalizer::VectorNormalizer;

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

pub struct OllamaEmbedder {
    endpoint: String,
    model: String,
}

impl OllamaEmbedder {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            endpoint: format!("{}/api/embeddings", base_url.trim_end_matches('/')),
            model: model.to_string(),
        }
    }

    pub fn generate_vector(&self, text: &str) -> Result<VectorData, String> {
        let req = EmbeddingRequest {
            model: &self.model,
            prompt: text,
        };

        let res: EmbeddingResponse = ureq::post(&self.endpoint)
            .send_json(&req)
            .map_err(|e| format!("Error comunicando con servicio de embeddings en {}: {}", self.endpoint, e))?
            .into_json()
            .map_err(|e| format!("Error parseando JSON de embeddings: {}", e))?;

        let mut aligned_vec = VectorData::default();
        let copy_limit = res.embedding.len().min(VECTOR_DIM);
        aligned_vec.raw[..copy_limit].copy_from_slice(&res.embedding[..copy_limit]);

        VectorNormalizer::normalize_l2(&mut aligned_vec);
        Ok(aligned_vec)
    }
}
