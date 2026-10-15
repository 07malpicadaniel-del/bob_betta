use serde::{Deserialize, Serialize};
use crate::config::CortexConfig;

#[derive(Serialize)]
struct CompletionRequest<'a> {
    prompt: &'a str,
    n_predict: i32,
    temperature: f32,
    id_slot: i32,
    cache_prompt: bool,
    stream: bool,
}

#[derive(Deserialize)]
struct CompletionResponse {
    content: String,
}

pub struct CortexEngine {
    config: CortexConfig,
}

impl CortexEngine {
    pub fn new(config: CortexConfig) -> Self {
        Self { config }
    }

    /// Envía el prompt al motor nativo llama-server reutilizando el KV Cache (cache_prompt = true)
    pub fn generate(&self, prompt: &str) -> Result<String, String> {
        let url = format!("{}/completion", self.config.endpoint.trim_end_matches('/'));
        let body = CompletionRequest {
            prompt,
            n_predict: self.config.n_predict,
            temperature: self.config.temperature,
            id_slot: self.config.slot_id,
            cache_prompt: true,
            stream: false,
        };

        let res: CompletionResponse = ureq::post(&url)
            .send_json(&body)
            .map_err(|e| format!("Fallo al conectar con el Córtex en {}: {}", url, e))?
            .into_json()
            .map_err(|e| format!("Error deserializando respuesta del Córtex: {}", e))?;

        Ok(res.content)
    }
}
