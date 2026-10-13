use std::path::Path;
use llama_cpp::{
    standard_sampler::StandardSampler,
    LlamaModel, LlamaParams, SessionParams,
};
use crate::config::ModelConfig;
use crate::kv_cache::KvCacheMonitor;

#[derive(thiserror::Error, Debug)]
pub enum CortexError {
    #[error("Error al cargar el modelo GGUF: {0}")]
    LoadError(String),
    #[error("Error al instanciar la sesión de contexto: {0}")]
    SessionError(String),
    #[error("Error durante la generación de tokens: {0}")]
    InferenceError(String),
}

pub struct CortexEngine {
    model: LlamaModel,
    config: ModelConfig,
    pub kv_monitor: KvCacheMonitor,
}

impl CortexEngine {
    pub fn load_from_file<P: AsRef<Path>>(path: P, config: ModelConfig) -> Result<Self, CortexError> {
        let mut model_params = LlamaParams::default();
        model_params.n_gpu_layers = config.n_gpu_layers;

        let model = LlamaModel::load_from_file(path.as_ref(), model_params)
            .map_err(|e| CortexError::LoadError(format!("{:?}", e)))?;

        let kv_monitor = KvCacheMonitor::new(config.context_size as usize);

        Ok(Self {
            model,
            config,
            kv_monitor,
        })
    }

    /// Generación de texto por streaming directo desde la GPU
    pub fn generate<F>(&mut self, prompt: &str, max_tokens: usize, mut on_token: F) -> Result<String, CortexError>
    where
        F: FnMut(&str),
    {
        let mut session_params = SessionParams::default();
        session_params.n_ctx = self.config.context_size;
        session_params.n_threads = self.config.threads;

        let mut session = self.model
            .create_session(session_params)
            .map_err(|e| CortexError::SessionError(format!("{:?}", e)))?;

        // Prefill
        session.advance_context(prompt)
            .map_err(|e| CortexError::InferenceError(format!("{:?}", e)))?;

        let mut output = String::new();
        let sampler = StandardSampler::default();

        // Desempaquetar el Result y convertir el handle en un iterador de Strings
        let completions = session
            .start_completing_with(sampler, max_tokens)
            .map_err(|e| CortexError::InferenceError(format!("{:?}", e)))?
            .into_strings();

        for token in completions {
            on_token(&token);
            output.push_str(&token);
        }

        Ok(output)
    }
}
