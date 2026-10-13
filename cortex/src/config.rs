use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub model_path: PathBuf,
    /// Número de capas a descargar a la GPU (99 para forzar todo el modelo a la RTX 5060 Ti)
    pub n_gpu_layers: u32,
    /// Tamaño de la ventana de contexto en tokens
    pub context_size: u32,
    /// Tamaño del lote para procesamiento en chunks (Prefill)
    pub batch_size: u32,
    /// Hilos de CPU para las tareas que no toquen la GPU
    pub threads: u32,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            n_gpu_layers: 99,
            context_size: 4096,
            batch_size: 512,
            threads: 8,
        }
    }
}
