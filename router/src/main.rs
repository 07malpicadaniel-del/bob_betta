use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cortex::{CortexConfig, CortexEngine};
use vector_db::causal_plane::{CausalHypergraph, CausalRelation, VectorThalamus};
use vector_db::common::types::IslandCategory;
use vector_db::ingestion::embedder::NativeEmbedder;
use vector_db::HippocampusEngine;

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Compresión Semántica: Descarta el monólogo interno <think>...</think> antes de consolidar en memoria
fn strip_thinking(response: &str) -> &str {
    if let Some(idx) = response.find("</think>") {
        response[idx + 8..].trim()
    } else {
        response.trim()
    }
}

#[tokio::main]
async fn main() {
    println!("==================================================");
    println!("  BOB Engine v2.0 — Hipocampo 4D + Doble Plano    ");
    println!("==================================================");

    let wal_path = "bob_memories.wal";
    let hippocampus = Arc::new(HippocampusEngine::new(
        16,             // M
        64,             // ef_construction
        86400,          // TTL 24h working memory
        0.0001,         // lambda decay base
        5,              // umbral de promoción topológica
        Some(wal_path), // Persistencia WAL (768 dims)
    ));

    let restored = hippocampus.recover_from_wal();
    if restored > 0 {
        println!("[*] Memorias restauradas desde WAL: {}", restored);
    }

    let _worker_handle = hippocampus.start_background_maintenance(Duration::from_secs(60));

    // 1. Inicializar Sentidos (Candle CPU 768-dim)
    println!("[*] Cargando motor sensorial nativo en CPU (768-dim)...");
    let embedder = NativeEmbedder::new().expect("[!] Error crítico al inicializar NativeEmbedder");
    println!("[✓] Motor sensorial activo en RAM (0% uso de VRAM).");

    // 2. Inicializar Tálamo Vectorial (<1 µs por producto punto SIMD)
    let thalamus = VectorThalamus::new(
        embedder.generate_vector("rust code cargo compiler bug struct function algorithm simd cuda").unwrap(),
        embedder.generate_vector("architecture axiom core rule leika aurora bob system hardware topology").unwrap(),
        embedder.generate_vector("current task testing status debug session today").unwrap(),
        embedder.generate_vector("conversation history remember user said yesterday personal context").unwrap(),
    );

    // 3. Inicializar Plano 2: Hipergrafo Causal Dirigido + LTP Decay
    let causal_graph = CausalHypergraph::new(0.0001, 2.5);

    // 4. Bootstrapping de Axiomas Base si el Hipocampo está vacío
    let now_boot = current_timestamp();
    if restored == 0 {
        println!("[*] Hipocampo vacío detectado. Inyectando Axiomas Fundamentales (Plano 1 HNSW + Plano 2 Causal)...");

        let ax0_text = "Axioma Hardware BOB: Estación Arch Linux (Hyprland) con AMD Ryzen 9 7900 (24 hilos, AVX-512), 64 GB RAM DDR5 y GPU NVIDIA RTX 5060 Ti (16 GB VRAM).".to_string();
        let ax0_vec = embedder.generate_vector(&ax0_text).unwrap();
        let (id0, _) = hippocampus.remember(ax0_text.clone(), ax0_vec, Some(IslandCategory::CoreRules), 2, now_boot);

        let ax1_text = "Axioma Lego 1 (Hipocampo 4D): Base de datos topológica en Rust con vectores de 768 dims alineados a 64 bytes, islas HNSW O(log N) y propagación causal O(E) en microsegundos.".to_string();
        let ax1_vec = embedder.generate_vector(&ax1_text).unwrap();
        let (id1, _) = hippocampus.remember(ax1_text.clone(), ax1_vec, Some(IslandCategory::CoreRules), 2, now_boot);

        let ax2_text = "Axioma Lego 2 y 3 (Córtex y Router): Inferencia local con Qwen 27B IQ4_XS (99 capas en GPU, KV Cache q8_0) orquestada por Router Tokio en Rust y embeddings Candle en CPU.".to_string();
        let ax2_vec = embedder.generate_vector(&ax2_text).unwrap();
        let (id2, _) = hippocampus.remember(ax2_text.clone(), ax2_vec, Some(IslandCategory::Code), 2, now_boot);

        let ax3_text = "Axioma Ecosistema Daniel: Holding maestro LEIKA, motor autónomo B2B AURORA, sistema de inventario Sertra y protocolo de tutoría Zero-Knowledge Quirón (Lego 5).".to_string();
        let ax3_vec = embedder.generate_vector(&ax3_text).unwrap();
        let (id3, _) = hippocampus.remember(ax3_text.clone(), ax3_vec, Some(IslandCategory::CoreRules), 2, now_boot);

        // Enlazar aristas causales dirigidas en el Plano 2
        causal_graph.link_nodes(id1, id0, CausalRelation::DependeDe, ax0_text.clone(), 1.0, now_boot);
        causal_graph.link_nodes(id2, id1, CausalRelation::Optimiza, ax1_text.clone(), 0.95, now_boot);
        causal_graph.link_nodes(id3, id2, CausalRelation::Implementa, ax2_text.clone(), 0.90, now_boot);
        causal_graph.record_validation(id0);
        causal_graph.record_validation(id1);

        println!("[✓] 4 Nodos Axiomáticos y 3 Aristas Causales cristalizados en el Hipocampo.");
    }

    // 5. Inicializar Córtex (auto-spawn llama-server en RTX 5060 Ti puerto 8085)
    println!("[*] Inicializando Córtex en GPU (cargando Qwen 27B en RTX 5060 Ti)...");
    let cortex = CortexEngine::new(CortexConfig::default()).expect("[!] Error crítico al levantar el Córtex");
    println!("[✓] Córtex 27B cargado en VRAM (99 capas, KV q8_0).");

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

        // 1. Vectorización nativa en CPU
        let start_emb = Instant::now();
        let query_vec = match embedder.generate_vector(input_clean) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[!] Error en embedder: {}", e);
                continue;
            }
        };
        let emb_elapsed = start_emb.elapsed();

        // 2. Enrutamiento en Tálamo Vectorial (<1 µs)
        let start_thal = Instant::now();
        let (routed_island, thal_sim) = thalamus.route_vector(&query_vec);
        let thal_elapsed = start_thal.elapsed();

        // 3. Recuperación Plano 1 (HNSW) + Plano 2 (Propagación Causal)
        let start_rec = Instant::now();
        let memory_context = hippocampus.recall(input_clean, query_vec.clone(), 4, now);
        let causal_links = causal_graph.propagate_causal_activation(1, now);
        let rec_elapsed = start_rec.elapsed();

        println!(
            "[*] Telemetría -> Ingesta: {:.2?} | Tálamo SIMD ({:?}, sim={:.2}): {:.2?} | Hipocampo 4D: {:.2?}",
            emb_elapsed, routed_island, thal_sim, thal_elapsed, rec_elapsed
        );

        let mut full_context = memory_context;
        if !causal_links.is_empty() {
            full_context.push_str("\n[Plano 2 - Aristas Causales Activas]:\n");
            for (rel, target_id, summary, energy) in causal_links {
                full_context.push_str(&format!(
                    "- {:?} -> Nodo #{}: {} (Energía={:.2})\n",
                    rel, target_id, summary, energy
                ));
            }
        }

        if !full_context.is_empty() {
            println!("{}", full_context.trim());
        }

        // 4. Ensamblaje de Prompt con RAG Topológico
        let final_prompt = if full_context.is_empty() {
            format!(
                "System: Eres Bob, un agente autónomo de ingeniería local creado por Daniel.\nUser: {}\nBob:",
                input_clean
            )
        } else {
            format!(
                "System: Eres Bob, un agente autónomo de ingeniería local creado por Daniel. Utiliza tu memoria del Hipocampo 4D para responder con precisión.\n{}\nUser: {}\nBob:",
                full_context, input_clean
            )
        };

        // 5. Inferencia en el Córtex (Lego 2)
        println!("[*] Razonando en Córtex (RTX 5060 Ti - 99 capas)...");
        let start_inf = Instant::now();
        match cortex.generate(&final_prompt) {
            Ok(response) => {
                println!("\n[Bob] ({:.2?}):\n{}", start_inf.elapsed(), response.trim());

                // 6. Compresión Semántica (purgar <think>) y Consolidación en Hipocampo + WAL
                let clean_answer = strip_thinking(&response);
                let memory_text = format!("Usuario: {} | Bob: {}", input_clean, clean_answer);
                match embedder.generate_vector(&memory_text) {
                    Ok(memory_vec) => {
                        thalamus.reinforce_centroid(routed_island, &memory_vec, 0.05);
                        let (node_id, cat) = hippocampus.remember(
                            memory_text,
                            memory_vec,
                            Some(routed_island),
                            1,
                            now,
                        );
                        println!(
                            "[*] Memoria consolidada -> Isla: {:?}, Nodo ID: {} (λ={:.6})",
                            cat,
                            node_id,
                            causal_graph.effective_lambda(node_id, cat == IslandCategory::CoreRules)
                        );
                    }
                    Err(e) => eprintln!("[!] Error consolidando vector en Hipocampo: {}", e),
                }
            }
            Err(e) => eprintln!("[!] Error en Córtex: {}", e),
        }
    }

    hippocampus.stop_background_maintenance();
    println!("\nSesión finalizada.");
}
