mod gatekeeper;
mod quiron;
mod thought_buffer;

use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cortex::kv_cache::KvCacheManager;
use cortex::{CortexConfig, CortexEngine};
use gatekeeper::{CodeLang, Gatekeeper};
use quiron::{QuironShield, TriggerReason};
use sandbox::FrictionChamber;
use thought_buffer::ThoughtBuffer;
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

#[tokio::main]
async fn main() {
    println!("======================================================================");
    println!("  BOB Engine v2.0 — Arquitectura Completa (Legos 1 a 5 Activos)       ");
    println!("  Comandos: /think | /rem | /quiron <txt> | /status | /kv-clear | exit");
    println!("======================================================================");

    let wal_path = "bob_memories.wal";
    let causal_path = "bob_causal.bin";
    let dpo_path = "bob_dpo_preferences.jsonl";

    let hippocampus = Arc::new(HippocampusEngine::new(
        16,
        64,
        86400,
        0.0001,
        5,
        Some(wal_path),
    ));

    let restored = hippocampus.recover_from_wal();
    if restored > 0 {
        println!("[*] Memorias HNSW restauradas desde WAL: {}", restored);
    }

    let _worker_handle = hippocampus.start_background_maintenance(Duration::from_secs(60));

    // 1. Sentidos (Candle CPU 768-dim)
    println!("[*] Cargando motor sensorial nativo en CPU (768-dim)...");
    let embedder = Arc::new(
        NativeEmbedder::new().expect("[!] Error crítico al inicializar NativeEmbedder"),
    );
    println!("[✓] Motor sensorial activo en RAM (0% uso de VRAM).");

    // 2. Tálamo Vectorial (<1 µs SIMD)
    let thalamus = Arc::new(VectorThalamus::new(
        embedder.generate_vector("rust code cargo compiler bug struct function algorithm simd cuda").unwrap(),
        embedder.generate_vector("architecture axiom core rule leika aurora bob system hardware topology").unwrap(),
        embedder.generate_vector("current task testing status debug session today").unwrap(),
        embedder.generate_vector("conversation history remember user said yesterday personal context").unwrap(),
    ));

    // 3. Plano 2: Hipergrafo Causal Dirigido
    let causal_graph = Arc::new(CausalHypergraph::new(0.0001, 2.5, Some(causal_path)));

    let now_boot = current_timestamp();
    if restored == 0 {
        println!("[*] Sembrando Axiomas Fundamentales (Plano 1 HNSW + Plano 2 Causal)...");

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

        causal_graph.link_nodes(id1, id0, CausalRelation::DependeDe, ax0_text.clone(), 1.0, now_boot);
        causal_graph.link_nodes(id2, id1, CausalRelation::Optimiza, ax1_text.clone(), 0.95, now_boot);
        causal_graph.link_nodes(id3, id2, CausalRelation::Implementa, ax2_text, 0.90, now_boot);
        causal_graph.record_validation(id0);
        causal_graph.record_validation(id1);
    }
    println!(
        "[✓] Plano 2 Causal activo ({} aristas dirigidas persistidas en `{}`).",
        causal_graph.total_edges(),
        causal_path
    );

    // 4. Inicializar Córtex (Lego 2), Gatekeeper (Lego 3), Cámara de Fricción (Lego 4) y Quirón (Lego 5)
    println!("[*] Inicializando Córtex en GPU (cargando Qwen 27B en RTX 5060 Ti)...");
    let cortex_cfg = CortexConfig::default();
    let kv_manager = KvCacheManager::new(&cortex_cfg.base_url());
    let cortex = CortexEngine::new(cortex_cfg).expect("[!] Error crítico al levantar el Córtex");
    println!("[✓] Córtex 27B cargado en VRAM (99 capas, KV q8_0).");

    let mut thought_buffer = ThoughtBuffer::new();
    let gatekeeper = Gatekeeper::new(dpo_path);
    let friction_chamber = FrictionChamber::new(3); // K = 3 intentos de auto-corrección determinista
    let mut quiron_shield = QuironShield::new(0.65);

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

        // Interceptación estricta de comandos REPL con '/' (evita que un typo como /tink llegue al LLM)
        if input_clean.starts_with('/') {
            if input_clean == "/think" || input_clean == "/tink" {
                match thought_buffer.last_thought() {
                    Some(t) => println!("\n--- [ÚLTIMO ÁRBOL DE PENSAMIENTO <think>] ---\n{}\n---------------------------------------------", t),
                    None => println!("[*] Aún no hay trazas de pensamiento en esta sesión."),
                }
            } else if input_clean == "/status" {
                println!(
                    "[*] Estado -> Aristas Causales: {} | Max Retries Fricción: K={} | WAL: `{}` | DPO: `{}`",
                    causal_graph.total_edges(),
                    friction_chamber.max_retries(),
                    wal_path,
                    dpo_path
                );
            } else if input_clean == "/kv-clear" {
                match kv_manager.erase_slot(0) {
                    Ok(res) => println!("[✓] KV Cache purgado en RTX 5060 Ti (Slot {}): {:?} tokens liberados.", res.id_slot, res.n_erased),
                    Err(e) => eprintln!("[!] {}", e),
                }
            } else if let Some(query_to_sanitize) = input_clean.strip_prefix("/quiron") {
                let sample = if query_to_sanitize.trim().is_empty() {
                    "Optimizar concurrencia en HippocampusEngine de LEIKA y AuroraEngine sobre /home/daniel/proyectos/bob_engine"
                } else {
                    query_to_sanitize.trim()
                };
                let packet = quiron_shield.sanitize_context(
                    sample,
                    IslandCategory::Code,
                    TriggerReason::InvocacionExplicita,
                );
                println!("\n┌────────────────────────────────────────────────────────────────────┐");
                println!("│ [LEGO 5 — ESCUDO QUIRÓN ZERO-KNOWLEDGE]                            │");
                println!("└────────────────────────────────────────────────────────────────────┘");
                println!("[*] Tutor Seleccionado: {:?}", packet.tutor);
                println!("[*] Motivo de Disparo:  {:?}", packet.reason);
                println!("[*] Símbolos Ofuscados en RAM (HashMap): {}", packet.symbol_table_size);
                println!("[*] System Prompt Socrático:\n    {}", packet.socratic_system_prompt);
                println!("[*] Payload Isomórfico Sanitizado (Lo único que saldría a la red):\n    \"{}\"", packet.sanitized_code_or_query);
                let restored_demo = quiron_shield.rehydrate(&packet.sanitized_code_or_query);
                println!("[✓] Prueba de Re-hidratación en RAM exitosa:\n    \"{}\"", restored_demo);
            } else if input_clean == "/rem" {
                println!("\n┌────────────────────────────────────────────────────────────────────┐");
                println!("│ [LEGO 4 — CICLO HYPER-REM (SUEÑO ALGORÍTMICO POR FRICCIÓN)]        │");
                println!("└────────────────────────────────────────────────────────────────────┘");
                let concept_a = "VectorData de 768 flotantes alineado a 64 bytes con normalización L2";
                let concept_b = "Ecuación de Potenciación a Largo Plazo (LTP) con decaimiento logarítmico según validaciones";
                println!("[*] Colisionando conceptos en Cámara de Fricción:\n    A: {}\n    B: {}", concept_a, concept_b);

                let rem_prompt = FrictionChamber::build_rem_dream_prompt(concept_a, concept_b);
                if let Ok(raw_rem) = cortex.generate(&rem_prompt) {
                    let parsed_rem = thought_buffer.process_output(&raw_rem);
                    let blocks = gatekeeper.scan_executable_blocks(&parsed_rem.clean_response);
                    if let Some(first_rust) = blocks.iter().find(|b| b.lang == CodeLang::Rust) {
                        let res = friction_chamber.verify_rust_code(&first_rust.code, 1);
                        if res.compiled_ok {
                            println!("[✓] Simulación Hyper-REM compilada y verificada con `rustc` (exit_code=0):");
                            println!("STDOUT:\n{}", res.stdout.trim());
                            let now_rem = current_timestamp();
                            let rem_memory = format!(
                                "[Axioma Cristalizado Hyper-REM]: Síntesis validada entre ({}) y ({}). Código verificado:\n{}",
                                concept_a, concept_b, first_rust.code
                            );
                            if let Ok(rem_vec) = embedder.generate_vector(&rem_memory) {
                                let (nid, cat) = hippocampus.remember(
                                    rem_memory,
                                    rem_vec,
                                    Some(IslandCategory::Code),
                                    2,
                                    now_rem,
                                );
                                causal_graph.record_validation(nid);
                                causal_graph.record_validation(nid);
                                println!(
                                    "[✓] Victoria Hyper-REM cristalizada en Hipocampo -> Isla: {:?}, Nodo ID: {} (λ={:.6})",
                                    cat,
                                    nid,
                                    causal_graph.effective_lambda(nid, false)
                                );
                            }
                        } else {
                            println!("[!] Sueño descartado por el Oráculo Determinista (fallo de compilación):\n{}", res.stderr.trim());
                        }
                    } else {
                        println!("[!] El sueño no produjo un bloque ```rust ejecutable.");
                    }
                }
            } else {
                println!("[!] Comando REPL desconocido: `{}`. Usa: /think | /rem | /quiron <txt> | /status | /kv-clear | exit", input_clean);
            }
            continue;
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

        // 3. Recuperación Plano 1 (HNSW) + Plano 2 (Causal)
        let start_rec = Instant::now();
        let memory_context = hippocampus.recall(input_clean, query_vec.clone(), 4, now);
        let seed_ids = CausalHypergraph::extract_seed_ids(&memory_context);
        let causal_links = causal_graph.propagate_from_seeds(&seed_ids, now);
        let rec_elapsed = start_rec.elapsed();

        println!(
            "[*] Telemetría -> Ingesta: {:.2?} | Tálamo SIMD ({:?}, sim={:.2}): {:.2?} | Hipocampo 4D: {:.2?}",
            emb_elapsed, routed_island, thal_sim, thal_elapsed, rec_elapsed
        );

        let mut full_context = memory_context;
        if !causal_links.is_empty() {
            full_context.push_str("\n[Plano 2 - Aristas Causales Activas]:\n");
            for (src_id, rel, target_id, summary, energy) in &causal_links {
                full_context.push_str(&format!(
                    "- (Nodo #{} {:?} Nodo #{}): {} (Energía={:.2})\n",
                    src_id, rel, target_id, summary, energy
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
            Ok(raw_response) => {
                let inf_elapsed = start_inf.elapsed();
                let parsed = thought_buffer.process_output(&raw_response);

                if parsed.thinking_trace.is_some() {
                    println!(
                        "[*] Buffer de Pensamiento Oculto: {} líneas / {} palabras evaluadas (escribe `/think` para inspeccionar).",
                        parsed.thought_lines, parsed.thought_words
                    );
                }

                println!("\n[Bob] ({:.2?}):\n{}", inf_elapsed, parsed.clean_response);

                // 6. Gatekeeper AST + Bucle de Auto-Corrección en Cámara de Fricción (Lego 4, K=3)
                let code_blocks = gatekeeper.scan_executable_blocks(&parsed.clean_response);
                let mut validated_by_sandbox = false;

                for (idx, block) in code_blocks.iter().enumerate() {
                    if let Some(outcome) = gatekeeper.authorize_and_execute(input_clean, idx, block, now) {
                        if outcome.approved {
                            println!(
                                "[*] Resultado Sandbox (exit code: {}):\nSTDOUT:\n{}\nSTDERR:\n{}",
                                outcome.exit_code,
                                outcome.stdout.trim(),
                                outcome.stderr.trim()
                            );

                            if outcome.exit_code == 0 {
                                validated_by_sandbox = true;
                                for &seed in &seed_ids {
                                    let n_val = causal_graph.record_validation(seed);
                                    println!("[✓] LTP Sináptico: Nodo Semilla #{} reforzado (validaciones={}).", seed, n_val);
                                }
                            } else if block.lang == CodeLang::Rust {
                                // Activar Bucle de Auto-Corrección Determinista en Cámara de Fricción (K=3)
                                let mut current_code = block.code.clone();
                                let mut current_err = outcome.stderr.clone();
                                let mut fixed = false;

                                for attempt in 1..=friction_chamber.max_retries() {
                                    println!(
                                        "\n[*] [CÁMARA DE FRICCIÓN] Auto-corrigiendo fallo de compilación (Intento {}/{})...",
                                        attempt,
                                        friction_chamber.max_retries()
                                    );
                                    let fix_prompt = FrictionChamber::build_self_correction_prompt(
                                        input_clean,
                                        &current_code,
                                        &current_err,
                                        attempt,
                                        friction_chamber.max_retries(),
                                    );

                                    if let Ok(fix_raw) = cortex.generate(&fix_prompt) {
                                        let fix_parsed = thought_buffer.process_output(&fix_raw);
                                        let fix_blocks = gatekeeper.scan_executable_blocks(&fix_parsed.clean_response);
                                        if let Some(rust_fix) = fix_blocks.iter().find(|b| b.lang == CodeLang::Rust) {
                                            let verify = friction_chamber.verify_rust_code(&rust_fix.code, attempt);
                                            if verify.compiled_ok {
                                                println!("[✓] Auto-corrección exitosa en intento #{}:", attempt);
                                                println!("```rust\n{}\n```", rust_fix.code);
                                                println!("STDOUT:\n{}", verify.stdout.trim());
                                                validated_by_sandbox = true;
                                                fixed = true;
                                                break;
                                            } else {
                                                current_code = rust_fix.code.clone();
                                                current_err = verify.stderr;
                                            }
                                        }
                                    }
                                }

                                // Si fallan los K=3 intentos, el Disparador Epistémico activa el Protocolo Quirón (Lego 5)
                                if !fixed {
                                    if let Some(reason) = quiron_shield.evaluate_trigger(
                                        thal_sim,
                                        friction_chamber.max_retries(),
                                        friction_chamber.max_retries(),
                                    ) {
                                        let packet = quiron_shield.sanitize_context(&current_code, routed_island, reason);
                                        println!("\n[!] Estancamiento en Cámara de Fricción detectado -> Escudo Quirón (Lego 5) preparado:");
                                        println!("    Tutor: {:?} | Símbolos protegidos en RAM: {}", packet.tutor, packet.symbol_table_size);
                                    }
                                }
                            }
                        }
                    }
                }

                // 7. Consolidación en Hipocampo + Enlace Causal Automático
                let memory_text = format!("Usuario: {} | Bob: {}", input_clean, parsed.clean_response);
                match embedder.generate_vector(&memory_text) {
                    Ok(memory_vec) => {
                        thalamus.reinforce_centroid(routed_island, &memory_vec, 0.05);
                        let (node_id, cat) = hippocampus.remember(
                            memory_text.clone(),
                            memory_vec,
                            Some(routed_island),
                            1,
                            now,
                        );

                        if validated_by_sandbox {
                            causal_graph.record_validation(node_id);
                        }

                        if let Some(&primary_seed) = seed_ids.first() {
                            let summary_short: String = memory_text.chars().take(120).collect();
                            causal_graph.link_nodes(
                                node_id,
                                primary_seed,
                                CausalRelation::DependeDe,
                                format!("{}...", summary_short),
                                0.90,
                                now,
                            );
                        }

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
