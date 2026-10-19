mod marathon;
mod ecosystem_seed;
mod server;
mod abe;
mod gatekeeper;
mod quiron;
mod thought_buffer;
mod tools;

use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use abe::BinaryScenarioTree;
use cortex::kv_cache::KvCacheManager;
use cortex::{CortexConfig, CortexEngine};
use gatekeeper::{CodeLang, Gatekeeper};
use quiron::{QuironShield, TriggerReason};
use sandbox::FrictionChamber;
use thought_buffer::ThoughtBuffer;
use tools::AgentToolkit;
use vector_db::causal_plane::{CausalHypergraph, CausalRelation, DynamicIslandManager, VectorThalamus};
use vector_db::common::types::IslandCategory;
use vector_db::ingestion::embedder::NativeEmbedder;
use vector_db::HippocampusEngine;

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn balance_rust_delimiters(mut code: String) -> String {
    let mut open_braces: i32 = 0;
    let mut in_string = false;
    let mut prev_char = '\0';

    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        for ch in line.chars() {
            if ch == '"' && prev_char != '\\' {
                in_string = !in_string;
            } else if !in_string {
                if ch == '{' {
                    open_braces += 1;
                } else if ch == '}' {
                    open_braces -= 1;
                }
            }
            prev_char = ch;
        }
    }

    while open_braces > 0 {
        code.push_str("\n}");
        open_braces -= 1;
    }
    code
}

pub fn extract_rust_from_prefill(text: &str, gatekeeper: &Gatekeeper) -> Option<String> {
    let blocks = gatekeeper.scan_executable_blocks(text);
    if let Some(b) = blocks.into_iter().find(|b| b.lang == CodeLang::Rust) {
        return Some(balance_rust_delimiters(b.code));
    }
    let trimmed = text.trim();
    if trimmed.contains("fn main") {
        let code_body = match trimmed.find("```") {
            Some(end_idx) => &trimmed[..end_idx],
            None => trimmed,
        };
        return Some(balance_rust_delimiters(code_body.trim().to_string()));
    }
    None
}

#[tokio::main]
async fn main() {
    println!("=========================================================================================");
    println!("  BOB Engine v2.0 — Agente Autónomo Local (Legos 1-5 + ABE + PC Tools + Web Search)     ");
    println!("  Comandos: /web <q> | /tools | /deep-think <reto> | /rem | /quiron <txt> | /think | exit");
    println!("=========================================================================================");

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
    println!("[✓] Córtex 27B cargado en VRAM (99 capas, KV q8_0, Slot-Save en RAM-disk).");

    let mut thought_buffer = ThoughtBuffer::new();
    let gatekeeper = Gatekeeper::new(dpo_path);
    let friction_chamber = FrictionChamber::new(3);
    let mut quiron_shield = QuironShield::new(0.65);

    // Sembrar contexto del ecosistema de Daniel y proyectos locales si el grafo tiene pocas aristas
    if causal_graph.total_edges() < 6 {
        println!("[*] Sembrando contexto maestro del ecosistema de Daniel + proyectos en Hipocampo 4D...");
        let seeded = ecosystem_seed::EcosystemSeeder::seed_daniel_ecosystem(&hippocampus, &causal_graph, &embedder, now_boot);
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
        let projs = ecosystem_seed::EcosystemSeeder::scan_and_ingest_local_projects(
            std::path::Path::new(&home_dir).join("proyectos").as_path(),
            &hippocampus,
            &causal_graph,
            &embedder,
            now_boot,
        );
        println!("[✓] Ecosistema sincronizado: {} axiomas maestros + {} repositorios locales indexados.", seeded, projs.len());
    }

    // Iniciar Servidor Puente para AXIS UI en 127.0.0.1:8086
    let custom_islands = Arc::new(DynamicIslandManager::new(Some("bob_custom_islands.bin")));
    let shared_state = Arc::new(server::BobSharedState {
        custom_islands: Arc::clone(&custom_islands),
        hippocampus: Arc::clone(&hippocampus),
        embedder: Arc::clone(&embedder),
        thalamus: Arc::clone(&thalamus),
        causal_graph: Arc::clone(&causal_graph),
        cortex: Arc::new(cortex),
        kv_manager: Arc::new(kv_manager),
        friction_chamber: Arc::new(FrictionChamber::new(3)),
        gatekeeper: Arc::new(Gatekeeper::new(dpo_path)),
        thought_buffer: tokio::sync::Mutex::new(ThoughtBuffer::new()),
        quiron_shield: tokio::sync::Mutex::new(QuironShield::new(0.65)),
    });
    let cortex = Arc::clone(&shared_state.cortex);
    let kv_manager = Arc::clone(&shared_state.kv_manager);

    tokio::spawn(server::start_axis_bridge_server(Arc::clone(&shared_state), 8086));

    // Si se lanzó con --daemon desde AXIS UI, mantenemos el demonio vivo sin bloquear por stdin
    if std::env::args().any(|a| a == "--daemon") {
        println!("[*] BOB ejecutándose en modo Daemon para AXIS UI (puerto 8086)...");
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    }

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

        if input_clean.starts_with('/') {
            if input_clean == "/think" || input_clean == "/tink" {
                match thought_buffer.last_thought() {
                    Some(t) => println!("\n--- [ÚLTIMO ÁRBOL DE PENSAMIENTO <think>] ---\n{}\n---------------------------------------------", t),
                    None => println!("[*] Aún no hay trazas de pensamiento en esta sesión."),
                }
            } else if input_clean == "/tools" {
                println!("\n┌────────────────────────────────────────────────────────────────────┐");
                println!("│ [HERRAMIENTAS NATIVAS DE AGENTE EN ARCH LINUX + INTERNET]          │");
                println!("└────────────────────────────────────────────────────────────────────┘");
                println!("{}", AgentToolkit::system_prompt_instructions());
            } else if let Some(web_q) = input_clean.strip_prefix("/web") {
                let q = web_q.trim();
                if q.is_empty() {
                    println!("[!] Uso: /web <consulta a buscar en internet>");
                } else {
                    println!("[*] Buscando en Internet: `{}`...", q);
                    let results = AgentToolkit::search_internet(q);
                    println!("\n{}\n", results);
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
            } else if let Some(problem_raw) = input_clean.strip_prefix("/deep-think") {
                let problem = if problem_raw.trim().is_empty() {
                    "Implementar un filtro de decaimiento exponencial y producto punto para 4 nodos de telemetría en Rust"
                } else {
                    problem_raw.trim()
                };

                let now_dt = current_timestamp();
                let dt_vec = embedder.generate_vector(problem).unwrap_or_default();
                let hippo_ctx = hippocampus.recall(problem, dt_vec, 3, now_dt);
                let seeds = CausalHypergraph::extract_seed_ids(&hippo_ctx);

                match BinaryScenarioTree::execute_deep_think(
                    problem,
                    &hippo_ctx,
                    &cortex,
                    &kv_manager,
                    &friction_chamber,
                    &gatekeeper,
                    &mut thought_buffer,
                    extract_rust_from_prefill,
                ) {
                    Ok(abe_res) => {
                        println!(
                            "\n[✓] [ESCENARIO IDEAL GANADOR]: {} ({}) | KV Trunk Compartido: {} tokens | Intentos: {}",
                            abe_res.winning_branch.name,
                            abe_res.winning_branch.strategy,
                            abe_res.kv_shared_bytes,
                            abe_res.winning_branch.attempts_used
                        );
                        println!("```rust\n{}\n```", abe_res.winning_branch.code);
                        println!("STDOUT:\n{}", abe_res.winning_branch.stdout.trim());

                        let abe_memory = format!(
                            "[Trayectoria Ideal ABE /deep-think]: Problema: {} | Estrategia Ganadora: {} (Podada: {}). Código:\n{}",
                            problem,
                            abe_res.winning_branch.strategy,
                            abe_res.pruned_branch_name,
                            abe_res.winning_branch.code
                        );
                        if let Ok(abe_vec) = embedder.generate_vector(&abe_memory) {
                            let (nid, cat) = hippocampus.remember(
                                abe_memory,
                                abe_vec,
                                Some(IslandCategory::Code),
                                2,
                                now_dt,
                            );
                            causal_graph.record_validation(nid);
                            causal_graph.record_validation(nid);

                            if let Some(&primary_seed) = seeds.first() {
                                causal_graph.link_nodes(
                                    nid,
                                    primary_seed,
                                    CausalRelation::Optimiza,
                                    format!("Trayectoria ABE optimizada para: {}", problem),
                                    0.98,
                                    now_dt,
                                );
                            }

                            println!(
                                "[✓] Trayectoria Ideal ABE cristalizada en Hipocampo -> Isla: {:?}, Nodo ID: {} (λ={:.6})",
                                cat,
                                nid,
                                causal_graph.effective_lambda(nid, false)
                            );
                        }
                    }
                    Err(e) => eprintln!("[!] Error en Árbol Binario de Escenarios: {}", e),
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
                println!("[*] Motivo de Disparo:  {}", packet.reason.description());
                println!("[*] Símbolos Ofuscados en RAM (HashMap): {}", packet.symbol_table_size);
                println!("[*] System Prompt Socrático:\n    {}", packet.socratic_system_prompt);
                println!("[*] Payload Isomórfico Sanitizado (Lo único que saldría a la red):\n    \"{}\"", packet.sanitized_code_or_query);
                let restored_demo = quiron_shield.rehydrate(&packet.sanitized_code_or_query);
                println!("[✓] Prueba de Re-hidratación en RAM exitosa:\n    \"{}\"", restored_demo);
            } else if input_clean == "/rem" {
                println!("\n┌────────────────────────────────────────────────────────────────────┐");
                println!("│ [LEGO 4 — CICLO HYPER-REM (MUESTREO DINÁMICO DEL HIPOCAMPO)]       │");
                println!("└────────────────────────────────────────────────────────────────────┘");

                let now_rem = current_timestamp();
                let probe_a = CausalHypergraph::random_hypersphere_probe();
                let probe_b = CausalHypergraph::random_hypersphere_probe();

                let raw_recall_a = hippocampus.recall("architecture axiom core rule", probe_a, 3, now_rem);
                let raw_recall_b = hippocampus.recall("rust code algorithm struct", probe_b, 3, now_rem);

                let mut candidates = CausalHypergraph::extract_nodes_with_text(&raw_recall_a);
                for (id, txt) in CausalHypergraph::extract_nodes_with_text(&raw_recall_b) {
                    if !candidates.iter().any(|(cid, _)| *cid == id) {
                        candidates.push((id, txt));
                    }
                }

                let (parent_a_id, concept_a) = candidates
                    .first()
                    .cloned()
                    .unwrap_or((0, "VectorData de 768 flotantes alineado a 64 bytes con normalización L2".to_string()));

                let (parent_b_id, concept_b) = candidates
                    .get(1)
                    .cloned()
                    .unwrap_or((1, "Ecuación de Potenciación a Largo Plazo (LTP) con decaimiento logarítmico".to_string()));

                let short_a: String = concept_a.chars().take(110).collect();
                let short_b: String = concept_b.chars().take(110).collect();

                println!(
                    "[*] Nodos extraídos dinámicamente del Hipocampo:\n    ├─ Nodo Padre #{}: {}...\n    └─ Nodo Padre #{}: {}...",
                    parent_a_id, short_a, parent_b_id, short_b
                );

                let rem_prompt = FrictionChamber::build_rem_dream_prompt(&short_a, &short_b);
                if let Ok(raw_rem) = cortex.generate(&rem_prompt) {
                    let parsed_rem = thought_buffer.process_output(&raw_rem);
                    if let Some(mut candidate_code) = extract_rust_from_prefill(&parsed_rem.clean_response, &gatekeeper) {
                        let mut solved = false;

                        for attempt in 1..=friction_chamber.max_retries() {
                            println!("[*] Validando en Oráculo Determinista `rustc` (Intento {}/{})...", attempt, friction_chamber.max_retries());
                            let res = friction_chamber.verify_rust_code(&candidate_code, attempt);

                            if res.compiled_ok {
                                println!("[✓] Simulación Hyper-REM compilada y verificada en intento #{} (exit_code=0):", attempt);
                                println!("```rust\n{}\n```", candidate_code);
                                println!("STDOUT:\n{}", res.stdout.trim());

                                let rem_memory = format!(
                                    "[Axioma Cristalizado Hyper-REM]: Síntesis entre Nodo #{} ({}) y Nodo #{} ({}). Código:\n{}",
                                    parent_a_id, short_a, parent_b_id, short_b, candidate_code
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

                                    causal_graph.link_nodes(
                                        nid,
                                        parent_a_id,
                                        CausalRelation::DependeDe,
                                        format!("Síntesis REM derivada de Nodo #{}", parent_a_id),
                                        0.95,
                                        now_rem,
                                    );
                                    causal_graph.link_nodes(
                                        nid,
                                        parent_b_id,
                                        CausalRelation::Optimiza,
                                        format!("Síntesis REM que optimiza Nodo #{}", parent_b_id),
                                        0.95,
                                        now_rem,
                                    );

                                    println!(
                                        "[✓] Victoria Hyper-REM cristalizada -> Isla: {:?}, Nodo ID: {} (λ={:.6}) | +2 Aristas Causales enlazadas.",
                                        cat,
                                        nid,
                                        causal_graph.effective_lambda(nid, false)
                                    );
                                }
                                solved = true;
                                break;
                            } else {
                                println!("[!] Fallo en intento #{}: {}", attempt, res.stderr.lines().next().unwrap_or("Error de sintaxis"));
                                if attempt < friction_chamber.max_retries() {
                                    let fix_prompt = FrictionChamber::build_self_correction_prompt(
                                        "Síntesis Hyper-REM dinámica",
                                        &candidate_code,
                                        &res.stderr,
                                        attempt + 1,
                                        friction_chamber.max_retries(),
                                    );
                                    if let Ok(fix_raw) = cortex.generate(&fix_prompt) {
                                        let fix_parsed = thought_buffer.process_output(&fix_raw);
                                        if let Some(new_code) = extract_rust_from_prefill(&fix_parsed.clean_response, &gatekeeper) {
                                            candidate_code = new_code;
                                        }
                                    }
                                }
                            }
                        }

                        if !solved {
                            println!("[!] Sueño descartado tras agotar los K={} intentos en la Cámara de Fricción.", friction_chamber.max_retries());
                        }
                    } else {
                        println!("[!] El sueño no produjo código Rust identificable.");
                    }
                }
            } else {
                println!("[!] Comando REPL desconocido: `{}`. Usa: /web <q> | /tools | /deep-think <reto> | /rem | /quiron <txt> | /think | /status | /kv-clear | exit", input_clean);
            }
            continue;
        }

        let now = current_timestamp();

        let start_emb = Instant::now();
        let query_vec = match embedder.generate_vector(input_clean) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[!] Error en embedder: {}", e);
                continue;
            }
        };
        let emb_elapsed = start_emb.elapsed();

        let start_thal = Instant::now();
        let (routed_island, thal_sim) = thalamus.route_vector(&query_vec);
        let thal_elapsed = start_thal.elapsed();

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

        // Reflejo Autónomo Pre-Inferencia (Herramientas de PC + Búsqueda en Internet)
        let reflex_calls = AgentToolkit::detect_reflex_tools(input_clean, thal_sim);
        let mut pre_executed_tools = false;
        if !reflex_calls.is_empty() {
            pre_executed_tools = true;
            full_context.push_str("\n### OBSERVACIONES REALES DE HERRAMIENTAS Y WEB EN VIVO:\n");
            for call in &reflex_calls {
                println!("[*] [REFLEJO AGÉNTICO] Ejecutando `{:?}`...", call);
                let res = AgentToolkit::execute_tool(call);
                println!("[✓] [{} (Aprobado={})] Datos obtenidos:\n{}", res.tool_name, res.approved, res.output.trim());
                full_context.push_str(&format!("[{}]:\n{}\n", res.tool_name, res.output));
            }
        }

        // Plantilla ChatML nativa de Qwen 3.8 para garantizar respuestas completas sin corte prematuro
        let system_block = format!(
            "Eres Bob, un agente autónomo de ingeniería local creado por Daniel en Arch Linux.\n\
            Responde siempre en español de forma técnica, directa y precisa.\n\
            Si en el contexto aparecen 'OBSERVACIONES REALES DE HERRAMIENTAS Y WEB EN VIVO', utiliza esos datos exactos para responder.\n\
            {}",
            AgentToolkit::system_prompt_instructions()
        );

        let user_block = if full_context.is_empty() {
            input_clean.to_string()
        } else {
            format!("Contexto y Datos en Vivo:\n{}\n\nPregunta de Daniel: {}", full_context, input_clean)
        };

        let final_prompt = format!(
            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            system_block, user_block
        );

        println!("[*] Razonando en Córtex (RTX 5060 Ti - 99 capas)...");
        let start_inf = Instant::now();
        match cortex.generate(&final_prompt) {
            Ok(raw_response) => {
                let inf_elapsed = start_inf.elapsed();
                let mut parsed = thought_buffer.process_output(&raw_response);

                if parsed.thinking_trace.is_some() {
                    println!(
                        "[*] Buffer de Pensamiento Oculto: {} líneas / {} palabras evaluadas (escribe `/think` para inspeccionar).",
                        parsed.thought_lines, parsed.thought_words
                    );
                }

                // Si Bob decidió invocar una herramienta adicional durante su respuesta, la ejecutamos en bucle ReAct
                if parsed.clean_response.contains("[TOOL:") {
                    let _ = pre_executed_tools;
                    let llm_tool_calls = AgentToolkit::parse_tool_calls(&parsed.clean_response);
                    if !llm_tool_calls.is_empty() {
                        let mut observations = String::new();
                        for call in &llm_tool_calls {
                            println!("[*] [AGENTE REACT] Ejecutando herramienta solicitada por Córtex: `{:?}`...", call);
                            let res = AgentToolkit::execute_tool(call);
                            println!("[✓] [{}] Datos capturados:\n{}", res.tool_name, res.output.trim());
                            observations.push_str(&format!("[{}]:\n{}\n", res.tool_name, res.output));
                        }

                        let followup_prompt = format!(
                            "<|im_start|>system\nEres Bob. Entrega la respuesta final en español usando los datos reales obtenidos.<|im_end|>\n\
                            <|im_start|>user\nPregunta: {}\n\nDatos Reales Capturados:\n{}<|im_end|>\n\
                            <|im_start|>assistant\n",
                            input_clean, observations
                        );
                        if let Ok(second_raw) = cortex.generate(&followup_prompt) {
                            parsed = thought_buffer.process_output(&second_raw);
                        }
                    }
                }

                println!("\n[Bob] ({:.2?}):\n{}", inf_elapsed, parsed.clean_response);

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
                                        if let Some(rust_fix_code) = extract_rust_from_prefill(&fix_parsed.clean_response, &gatekeeper) {
                                            let verify = friction_chamber.verify_rust_code(&rust_fix_code, attempt);
                                            if verify.compiled_ok {
                                                println!("[✓] Auto-corrección exitosa en intento #{}:", attempt);
                                                println!("```rust\n{}\n```", rust_fix_code);
                                                println!("STDOUT:\n{}", verify.stdout.trim());
                                                validated_by_sandbox = true;
                                                fixed = true;
                                                break;
                                            } else {
                                                current_code = rust_fix_code;
                                                current_err = verify.stderr;
                                            }
                                        }
                                    }
                                }

                                if !fixed {
                                    if let Some(reason) = quiron_shield.evaluate_trigger(
                                        thal_sim,
                                        friction_chamber.max_retries(),
                                        friction_chamber.max_retries(),
                                    ) {
                                        let packet = quiron_shield.sanitize_context(&current_code, routed_island, reason);
                                        println!("\n[!] Estancamiento en Cámara de Fricción detectado -> Escudo Quirón (Lego 5) preparado:");
                                        println!("    Motivo: {} | Tutor: {:?} | Símbolos protegidos en RAM: {}", packet.reason.description(), packet.tutor, packet.symbol_table_size);
                                    }
                                }
                            }
                        }
                    }
                }

                // Solo consolidamos en el Hipocampo si la respuesta no está vacía
                if !parsed.clean_response.trim().is_empty() {
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
            }
            Err(e) => eprintln!("[!] Error en Córtex: {}", e),
        }
    }

    hippocampus.stop_background_maintenance();
    println!("\nSesión finalizada.");
}
