use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotActionResponse {
    pub id_slot: i32,
    pub filename: Option<String>,
    pub n_saved: Option<usize>,
    pub n_restored: Option<usize>,
    pub n_erased: Option<usize>,
}

#[derive(Serialize)]
struct SlotSaveRestoreReq<'a> {
    filename: &'a str,
}

/// Orquestador del KV Cache y Prefix Tree Sharing sobre llama-server (/slots)
pub struct KvCacheManager {
    base_url: String,
}

impl KvCacheManager {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Guarda el estado actual del KV Cache de un slot en disco/RAM-disk para bifurcar ramas (ABE)
    pub fn save_slot(&self, slot_id: i32, filename: &str) -> Result<SlotActionResponse, String> {
        let url = format!("{}/slots/{}?action=save", self.base_url, slot_id);
        let req = SlotSaveRestoreReq { filename };

        ureq::post(&url)
            .timeout(Duration::from_secs(10))
            .send_json(&req)
            .map_err(|e| format!("Error guardando KV Cache en slot {}: {}", slot_id, e))?
            .into_json()
            .map_err(|e| format!("Error parseando respuesta de slot save: {}", e))
    }

    /// Restaura una rama previamente guardada en el KV Cache sin recalcular el prefill
    pub fn restore_slot(&self, slot_id: i32, filename: &str) -> Result<SlotActionResponse, String> {
        let url = format!("{}/slots/{}?action=restore", self.base_url, slot_id);
        let req = SlotSaveRestoreReq { filename };

        ureq::post(&url)
            .timeout(Duration::from_secs(10))
            .send_json(&req)
            .map_err(|e| format!("Error restaurando KV Cache en slot {}: {}", slot_id, e))?
            .into_json()
            .map_err(|e| format!("Error parseando respuesta de slot restore: {}", e))
    }

    /// Purga inmediatamente los tensores del KV Cache del slot activo en la RTX 5060 Ti
    pub fn erase_slot(&self, slot_id: i32) -> Result<SlotActionResponse, String> {
        let url = format!("{}/slots/{}?action=erase", self.base_url, slot_id);

        ureq::post(&url)
            .timeout(Duration::from_secs(5))
            .call()
            .map_err(|e| format!("Error purgando KV Cache en slot {}: {}", slot_id, e))?
            .into_json()
            .map_err(|e| format!("Error parseando respuesta de slot erase: {}", e))
    }
}
