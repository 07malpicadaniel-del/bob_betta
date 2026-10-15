use candle_core::{Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use hf_hub::{api::sync::Api, Repo, RepoType};
use tokenizers::{PaddingParams, Tokenizer};

use crate::common::types::{VectorData, VECTOR_DIM};
use super::normalizer::VectorNormalizer;

pub struct NativeEmbedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl NativeEmbedder {
    pub fn new() -> Result<Self, String> {
        let device = Device::Cpu;
        let model_id = "BAAI/bge-base-en-v1.5";

        let api = Api::new().map_err(|e| format!("Error inicializando HF Hub API: {}", e))?;
        let repo = api.repo(Repo::new(model_id.to_string(), RepoType::Model));

        let config_path = repo
            .get("config.json")
            .map_err(|e| format!("Error obteniendo config.json: {}", e))?;
        let tokenizer_path = repo
            .get("tokenizer.json")
            .map_err(|e| format!("Error obteniendo tokenizer.json: {}", e))?;
        let weights_path = repo
            .get("model.safetensors")
            .map_err(|e| format!("Error obteniendo model.safetensors: {}", e))?;

        let config_str = std::fs::read_to_string(config_path)
            .map_err(|e| format!("Error leyendo config.json: {}", e))?;
        let config: Config = serde_json::from_str(&config_str)
            .map_err(|e| format!("Error parseando config BERT: {}", e))?;

        let mut tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("Error cargando Tokenizer: {}", e))?;
        let _ = tokenizer.with_padding(Some(PaddingParams::default()));

        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], DTYPE, &device)
                .map_err(|e| format!("Error mapeando safetensors en RAM: {}", e))?
        };

        let model = BertModel::load(vb, &config)
            .map_err(|e| format!("Error construyendo grafo BertModel: {}", e))?;

        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }

    /// Genera el tensor de 768 dimensiones truncando de forma determinista a 512 tokens
    pub fn generate_vector(&self, text: &str) -> Result<VectorData, String> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("Error tokenizando entrada: {}", e))?;

        let full_ids = encoding.get_ids();
        let full_type_ids = encoding.get_type_ids();

        // Límite físico de positional embeddings en BAAI/bge-base-en-v1.5 (512 tokens)
        let max_tokens = full_ids.len().min(512);
        let ids = &full_ids[..max_tokens];
        let type_ids = &full_type_ids[..max_tokens];

        let token_ids = Tensor::new(ids, &self.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| format!("Error creando tensor token_ids: {}", e))?;
        let token_type_ids = Tensor::new(type_ids, &self.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| format!("Error creando tensor token_type_ids: {}", e))?;

        let embeddings = self
            .model
            .forward(&token_ids, &token_type_ids, None)
            .map_err(|e| format!("Error en forward pass de embeddings: {}", e))?;

        let (_n_sentence, n_tokens, _hidden_size) = embeddings
            .dims3()
            .map_err(|e| format!("Dimensiones inesperadas en salida BERT: {}", e))?;
        let pooled = (embeddings.sum(1).map_err(|e| e.to_string())? / (n_tokens as f64))
            .map_err(|e| format!("Error en mean pooling: {}", e))?;
        let pooled = pooled
            .squeeze(0)
            .map_err(|e| format!("Error en squeeze(0): {}", e))?;

        let raw_floats: Vec<f32> = pooled
            .to_vec1()
            .map_err(|e| format!("Error extrayendo Vec<f32>: {}", e))?;

        let mut aligned_vec = VectorData::default();
        let copy_limit = raw_floats.len().min(VECTOR_DIM);
        aligned_vec.raw[..copy_limit].copy_from_slice(&raw_floats[..copy_limit]);

        VectorNormalizer::normalize_l2(&mut aligned_vec);

        Ok(aligned_vec)
    }
}
