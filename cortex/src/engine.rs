use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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
    server_process: Option<Child>,
}

impl CortexEngine {
    pub fn new(config: CortexConfig) -> Result<Self, String> {
        let mut engine = Self {
            config,
            server_process: None,
        };

        if !engine.is_server_ready() {
            engine.spawn_server()?;
            engine.wait_until_ready(Duration::from_secs(45))?;
        }

        Ok(engine)
    }

    fn is_server_ready(&self) -> bool {
        let health_url = format!("{}/health", self.config.base_url());
        match ureq::get(&health_url).timeout(Duration::from_millis(500)).call() {
            Ok(resp) => resp.status() == 200,
            Err(_) => false,
        }
    }

    fn spawn_server(&mut self) -> Result<(), String> {
        if !self.config.server_bin.exists() {
            return Err(format!(
                "No se encontró el binario llama-server en: {:?}",
                self.config.server_bin
            ));
        }
        if !self.config.model_path.exists() {
            return Err(format!(
                "No se encontró el modelo GGUF en: {:?}",
                self.config.model_path
            ));
        }

        let _ = std::fs::create_dir_all(&self.config.slot_save_dir);

        let child = Command::new(&self.config.server_bin)
            .arg("-m")
            .arg(&self.config.model_path)
            .arg("-ngl")
            .arg(self.config.n_gpu_layers.to_string())
            .arg("-np")
            .arg("1")
            .arg("-ctk")
            .arg("q8_0")
            .arg("-ctv")
            .arg("q8_0")
            .arg("-fa")
            .arg("on")
            .arg("-c")
            .arg(self.config.ctx_size.to_string())
            .arg("-b")
            .arg(self.config.batch_size.to_string())
            .arg("--slot-save-path")
            .arg(&self.config.slot_save_dir)
            .arg("--port")
            .arg(self.config.port.to_string())
            .arg("--host")
            .arg(&self.config.host)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Fallo al lanzar el subproceso llama-server: {}", e))?;

        self.server_process = Some(child);
        Ok(())
    }

    fn wait_until_ready(&mut self, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Some(child) = &mut self.server_process {
                if let Ok(Some(status)) = child.try_wait() {
                    return Err(format!(
                        "El subproceso llama-server abortó prematuramente con estado: {}",
                        status
                    ));
                }
            }

            if self.is_server_ready() {
                return Ok(());
            }

            thread::sleep(Duration::from_millis(250));
        }

        Err("Tiempo de espera agotado esperando a que el Córtex cargue el modelo en VRAM".to_string())
    }

    pub fn generate(&self, prompt: &str) -> Result<String, String> {
        self.generate_branch(prompt, self.config.temperature)
    }

    /// Genera una rama lógica con temperatura personalizada reutilizando el KV Cache (Prefix Sharing)
    pub fn generate_branch(&self, prompt: &str, temperature: f32) -> Result<String, String> {
        let url = format!("{}/completion", self.config.base_url());
        let body = CompletionRequest {
            prompt,
            n_predict: self.config.n_predict,
            temperature,
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

impl Drop for CortexEngine {
    fn drop(&mut self) {
        if let Some(mut child) = self.server_process.take() {
            println!("[*] Apagando subproceso Córtex y liberando VRAM...");
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
