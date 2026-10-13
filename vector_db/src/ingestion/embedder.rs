use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use crate::common::types::{VectorData, VECTOR_DIM};
use super::normalizer::VectorNormalizer;

pub struct NativeEmbedder {
    model: TextEmbedding,
}

impl NativeEmbedder {
    pub fn new() -> Result<Self, String> {
        // Carga Nomic-Embed-Text v1.5 directamente en el proceso de Rust.
        // La primera vez lo descargará automáticamente a ~/.cache/fastembed/
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::NomicEmbedTextV15)
                .with_show_download_progress(true),
        )
        .map_err(|e| format!("Error inicializando FastEmbed nativo: {}", e))?;

        Ok(Self { model })
    }

    /// Vectoriza en memoria sin salir del proceso, trunca/rellena y normaliza
    pub fn generate_vector(&self, text: &str) -> Result<VectorData, String> {
        let embeddings = self.model
            .embed(vec![text], None)
            .map_err(|e| format!("Error en inferencia de ONNX: {}", e))?;

        let embedding = &embeddings[0]; // Obtenemos el vector de la primera (y única) cadena

        let mut aligned_vec = VectorData::default();
        let copy_limit = embedding.len().min(VECTOR_DIM);
        aligned_vec.raw[..copy_limit].copy_from_slice(&embedding[..copy_limit]);

        // Asegurar que la magnitud sea 1.0 para comparaciones SIMD (HNSW)
        VectorNormalizer::normalize_l2(&mut aligned_vec);

        Ok(aligned_vec)
    }
}
