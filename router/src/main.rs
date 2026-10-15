use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cortex::{CortexConfig, CortexEngine};
use vector_db::ingestion::embedder::OllamaEmbedder;
use vector_db::HippocampusEngine;

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[tokio::main]
async fn main() {
    println!("==================================================");
    println!("  BOB Engine v2.0 — Lego 3 Router (100% Rust)     ");
    println!("==================================================");

    let wal_path = "bob_memories.wal";
    let hippocampus = Arc::new(HippocampusEngine::new(
        16,             // M
        64,             // ef_construction
        86400,          // TTL 24h working memory
        0.0001,         // lambda decay
        5,              // umbral de promoción topológica
        Some(wal_path), // Persistencia WAL
    ));

    let restored = hippocampus.recover_from_wal();
    if restored > 0 {
        println!("[*] Memorias restauradas desde WAL: {}", restored);
    }

    let _worker_handle = hippocampus.start_background_maintenance(Duration::from_secs(60));

    // Lego 2: Córtex conectado a llama-server en el puerto 8085 con KV Cache persistente
    let cortex = CortexEngine::new(CortexConfig::default());

    // Ingesta sensorial (será reemplazada en Fase 2 por 768-dim nativo)
    let embedder = OllamaEmbedder::new("http://127.0.0.1:11434", "nomic-embed-text");

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

        // 1. Vectorización de la consulta
        let query_vec = match embedder.generate_vector(input_clean) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[!] {}", e);
                continue;
            }
        };

        // 2. Recuperación topológica en el Hipocampo (Lego 1)
        let memory_context = hippocampus.recall(input_clean, query_vec, 3, now);
        if !memory_context.is_empty() {
            println!("{}", memory_context);
        }

        // 3. Ensamblaje de Prompt con RAG
        let final_prompt = if memory_context.is_empty() {
            format!(
                "System: Eres Bob, un agente autónomo de ingeniería.\nUser: {}\nBob:",
                input_clean
            )
        } else {
            format!(
                "System: Eres Bob, un agente autónomo de ingeniería.\n{}\nUser: {}\nBob:",
                memory_context, input_clean
            )
        };

        // 4. Inferencia en el Córtex (Lego 2)
        println!("[*] Razonando en Córtex (RTX 5060 Ti)...");
        match cortex.generate(&final_prompt) {
            Ok(response) => {
                println!("\n[Bob]:\n{}", response.trim());

                // 5. Consolidación en Hipocampo y WAL
                let memory_text = format!("Usuario: {} | Bob: {}", input_clean, response.trim());
                if let Ok(memory_vec) = embedder.generate_vector(&memory_text) {
                    let (node_id, cat) = hippocampus.remember(
                        memory_text,
                        memory_vec,
                        None,
                        1,
                        now,
                    );
                    println!("[*] Memoria consolidada -> Isla: {:?}, Nodo ID: {}", cat, node_id);
                }
            }
            Err(e) => eprintln!("[!] Error en Córtex: {}", e),
        }
    }

    hippocampus.stop_background_maintenance();
    println!("\nSesión finalizada.");
}
