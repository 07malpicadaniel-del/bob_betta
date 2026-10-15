use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct CortexConfig {
    pub host: String,
    pub port: u16,
    pub n_predict: i32,
    pub temperature: f32,
    pub slot_id: i32,
    pub n_gpu_layers: u32,
    pub ctx_size: u32,
    pub batch_size: u32,
    pub server_bin: PathBuf,
    pub model_path: PathBuf,
}

impl Default for CortexConfig {
    fn default() -> Self {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("No se pudo obtener la raíz del workspace")
            .to_path_buf();

        Self {
            host: "127.0.0.1".to_string(),
            port: 8085,
            n_predict: 1024,
            temperature: 0.2,
            slot_id: 0,
            n_gpu_layers: 99,
            ctx_size: 4096,
            batch_size: 512,
            server_bin: workspace_root.join("llama.cpp/build/bin/llama-server"),
            model_path: workspace_root.join("models/Qwen3.8-27B-Uncensored-noMTP-IQ4_XS.gguf"),
        }
    }
}

impl CortexConfig {
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}
