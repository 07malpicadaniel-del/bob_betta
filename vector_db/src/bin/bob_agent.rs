use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
// 1. Cambiamos la importación aquí
use vector_db::ingestion::embedder::NativeEmbedder;
use vector_db::HippocampusEngine;

#[derive(Serialize)]
struct CompletionRequest<'a> {
    prompt: &'a str,
    n_predict: i32,
    stream: bool,
}

#[derive(Deserialize)]
struct CompletionResponse {
    content: String,
}

fn query_bob(prompt: &str) -> Result<String, String> {
    // Apuntamos al Lego 2 (llama-server compilado nativamente)
    let endpoint = "http://127.0.0.1:8085/completion";
    let body = CompletionRequest {
        prompt,
        n_predict: 256,
        stream: false,
    };

    let res: CompletionResponse = ureq::post(endpoint)
        .send_json(&body)
        .map_err(|e| format!("Fallo al conectar con Córtex (Lego 2): {}", e))?
        .into_json()
        .map_err(|e| format!("Error deserializando respuesta: {}", e))?;

    Ok(res.content)
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn main() {
    println!("==================================================");
    println!("  Iniciando Bob Agent (100% Rust + Bare Metal)    ");
    println!("==================================================");

    let wal_path = "bob_memories.wal";
    let engine = Arc::new(HippocampusEngine::new(
        16, 64, 86400, 0.0001, 5, Some(wal_path),
    ));

    let restored = engine.recover_from_wal();
    if restored > 0 {
        println!("[*] Memorias restauradas desde disco: {}", restored);
    }

    let _worker_handle = engine.start_background_maintenance(Duration::from_secs(60));

    // 2. Inicializamos el motor nativo de ONNX (Tardará unos segundos la primera vez si descarga pesos)
    println!("[*] Inicializando Córtex sensorial (Nomic-Embed nativo)...");
    let embedder = NativeEmbedder::new().expect("[!] Fallo fatal al cargar FastEmbed");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("\n[Tú] > ");
        stdout.flush().unwrap();

        let mut user_input = String::new();
        if stdin.read_line(&mut user_input).is_err() || user_input.trim().is_empty() {
            continue;
        }

        let input_clean = user_input.trim();
        if input_clean == "exit" || input_clean == "quit" {
            break;
        }

        let now = current_timestamp();

        let query_vec = match embedder.generate_vector(input_clean) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[!] {}", e);
                continue;
            }
        };

        let memory_context = engine.recall(input_clean, query_vec.clone(), 3, now);
        if !memory_context.is_empty() {
            println!("--- MEMORIA --- \n{}\n---------------", memory_context);
        }

        let final_prompt = if memory_context.is_empty() {
            format!("System: Eres Bob, un agente autónomo de ingeniería.\nUser: {}\nBob:", input_clean)
        } else {
            format!(
                "System: Eres Bob, un agente autónomo de ingeniería.\n{}\nUser: {}\nBob:",
                memory_context, input_clean
            )
        };

        match query_bob(&final_prompt) {
            Ok(response) => {
                println!("\n[Bob]:\n{}", response.trim());

                let memory_text = format!("Usuario: {} | Bob: {}", input_clean, response.trim());
                if let Ok(memory_vec) = embedder.generate_vector(&memory_text) {
                    let (node_id, cat) = engine.remember(memory_text, memory_vec, None, 1, now);
                    println!("[*] Memoria consolidada -> Isla: {:?}, Nodo ID: {}", cat, node_id);
                }
            }
            Err(e) => eprintln!("[!] Error en inferencia: {}", e),
        }
    }

    engine.stop_background_maintenance();
    println!("\nSesión finalizada.");
}
