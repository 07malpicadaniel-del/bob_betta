#[derive(Debug, Clone)]
pub struct CortexConfig {
    pub endpoint: String,
    pub n_predict: i32,
    pub temperature: f32,
    pub slot_id: i32,
}

impl Default for CortexConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8085".to_string(),
            n_predict: 512,
            temperature: 0.2,
            slot_id: 0,
        }
    }
}
