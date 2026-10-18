use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use cortex::kv_cache::KvCacheManager;
use cortex::serde::{Deserialize, Serialize};
use cortex::serde_json;
use cortex::CortexEngine;
use sandbox::FrictionChamber;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use vector_db::causal_plane::{
    CausalHypergraph, CausalRelation, CustomIslandSummary, DynamicIslandManager, VectorThalamus,
};
use vector_db::ingestion::embedder::NativeEmbedder;
use vector_db::HippocampusEngine;

use crate::abe::BinaryScenarioTree;
use crate::ecosystem_seed::EcosystemSeeder;
use crate::extract_rust_from_prefill;
use crate::gatekeeper::{CodeLang, Gatekeeper};
use crate::quiron::{QuironShield, TriggerReason};
use crate::thought_buffer::ThoughtBuffer;
use crate::tools::{AgentToolCall, AgentToolkit};

pub struct BobSharedState {
    pub hippocampus: Arc<HippocampusEngine>,
    pub embedder: Arc<NativeEmbedder>,
    pub thalamus: Arc<VectorThalamus>,
    pub causal_graph: Arc<CausalHypergraph>,
    pub custom_islands: Arc<DynamicIslandManager>,
    pub cortex: Arc<CortexEngine>,
    pub kv_manager: Arc<KvCacheManager>,
    pub friction_chamber: Arc<FrictionChamber>,
    pub gatekeeper: Arc<Gatekeeper>,
    pub thought_buffer: Mutex<ThoughtBuffer>,
    pub quiron_shield: Mutex<QuironShield>,
}

static RECENT_TURNS: std::sync::OnceLock<Mutex<Vec<(String, String, String)>>> =
    std::sync::OnceLock::new();

fn get_recent_turns() -> &'static Mutex<Vec<(String, String, String)>> {
    RECENT_TURNS.get_or_init(|| Mutex::new(Vec::new()))
}

#[derive(Deserialize)]
#[serde(crate = "cortex::serde")]
struct ApiChatRequest {
    message: String,
    mode: Option<String>,
    axis_context: Option<String>,
}

#[derive(Serialize)]
#[serde(crate = "cortex::serde")]
struct ApiChatResponse {
    ok: bool,
    answer: String,
    thinking: Option<String>,
    island: String,
    active_custom_island: Option<String>,
    custom_islands: Vec<CustomIslandSummary>,
    similarity: f32,
    ingest_ms: f64,
    thalamus_us: f64,
    hippocampus_us: f64,
    total_ms: f64,
    causal_edges_total: usize,
    tools_used: Vec<String>,
}

#[derive(Serialize)]
#[serde(crate = "cortex::serde")]
struct ApiStatusResponse {
    online: bool,
    model: String,
    causal_edges: usize,
    sys_info: String,
    custom_islands: Vec<CustomIslandSummary>,
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}


fn hydrate_visual_graph(st: &Arc<BobSharedState>) {
    if st.custom_islands.base_nodes_count() > 0 {
        return;
    }
    let now = now_ts();
    let probes = [
        "Axioma Hardware BOB Arch Linux Ryzen RTX 5060 Ti LEIKA AURORA",
        "Proyecto Local Indexado Rust Tauri React Kotlin Python",
        "Trayectoria Ideal ABE Hyper-REM código rustc",
        "Usuario Daniel conversación memoria",
    ];
    for p in probes {
        if let Ok(p_vec) = st.embedder.generate_vector(p) {
            let recalled = st.hippocampus.recall(p, p_vec, 40, now);
            for (nid, text) in CausalHypergraph::extract_nodes_with_text(&recalled) {
                if let Ok(n_vec) = st.embedder.generate_vector(&text) {
                    let (isl_cat, _) = st.thalamus.route_vector(&n_vec);
                    st.custom_islands.register_base_node(
                        nid,
                        &format!("{:?}", isl_cat),
                        text,
                        &n_vec,
                        now,
                    );
                }
            }
        }
    }
}

pub async fn start_axis_bridge_server(state: Arc<BobSharedState>, port: u16) {
    hydrate_visual_graph(&state);
    let addr = format!("127.0.0.1:{}", port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => {
            println!("[✓] Puente UI AXIS activo en http://{} (JSON API lista).", addr);
            l
        }
        Err(e) => {
            eprintln!("[!] No se pudo abrir el puerto {} para AXIS UI: {}", port, e);
            return;
        }
    };

    loop {
        if let Ok((mut stream, _)) = listener.accept().await {
            let st = Arc::clone(&state);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 131072];
                let Ok(n) = stream.read(&mut buf).await else {
                    return;
                };
                if n == 0 {
                    return;
                }

                let req_str = String::from_utf8_lossy(&buf[..n]);
                let first_line = req_str.lines().next().unwrap_or("");
                let body_start = req_str.find("\r\n\r\n").map(|i| i + 4).unwrap_or(n);
                let body = &req_str[body_start..];

                if first_line.starts_with("OPTIONS ") {
                    let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\n\r\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }

                let json_string = if first_line.starts_with("GET /graph") {
                    hydrate_visual_graph(&st);
                    let snap = st.custom_islands.build_graph_snapshot(&st.causal_graph);
                    serde_json::to_string(&snap).unwrap_or_else(|_| "{}".to_string())
                } else if first_line.starts_with("GET /status") {
                    let status = ApiStatusResponse {
                        online: true,
                        model: "Qwen3.8-27B-Uncensored-IQ4_XS (99 capas RTX 5060 Ti)".to_string(),
                        causal_edges: st.causal_graph.total_edges(),
                        sys_info: AgentToolkit::execute_tool(&AgentToolCall::SysInfo).output,
                        custom_islands: st.custom_islands.list_islands(),
                    };
                    serde_json::to_string(&status).unwrap_or_else(|_| "{}".to_string())
                } else if first_line.starts_with("POST /sync-projects") {
                    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
                    let projects_dir = PathBuf::from(home).join("proyectos");
                    let now = now_ts();
                    let base_count = EcosystemSeeder::seed_daniel_ecosystem(
                        &st.hippocampus,
                        &st.causal_graph,
                        &st.embedder,
                        now,
                    );
                    let scanned = EcosystemSeeder::scan_and_ingest_local_projects(
                        &projects_dir,
                        &st.hippocampus,
                        &st.causal_graph,
                        &st.embedder,
                        now,
                    );
                    serde_json::json!({
                        "ok": true,
                        "axioms_seeded": base_count,
                        "projects_indexed": scanned,
                        "causal_edges": st.causal_graph.total_edges(),
                        "custom_islands": st.custom_islands.list_islands()
                    })
                    .to_string()
                } else if first_line.starts_with("POST /chat") {
                    let resp_struct = handle_axis_chat(&st, body).await;
                    serde_json::to_string(&resp_struct).unwrap_or_else(|_| "{}".to_string())
                } else {
                    "{\"error\":\"endpoint desconocido\"}".to_string()
                };

                let http_resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
                    json_string.len(),
                    json_string
                );
                let _ = stream.write_all(http_resp.as_bytes()).await;
            });
        }
    }
}

async fn handle_axis_chat(st: &Arc<BobSharedState>, body: &str) -> ApiChatResponse {
    let total_start = Instant::now();
    let parsed_req: ApiChatRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => {
            return ApiChatResponse {
                ok: false,
                answer: format!("Error parseando JSON en Bob Daemon: {}", e),
                thinking: None,
                island: "Error".to_string(),
                active_custom_island: None,
                custom_islands: st.custom_islands.list_islands(),
                similarity: 0.0,
                ingest_ms: 0.0,
                thalamus_us: 0.0,
                hippocampus_us: 0.0,
                total_ms: 0.0,
                causal_edges_total: st.causal_graph.total_edges(),
                tools_used: vec![],
            };
        }
    };

    let raw_msg = parsed_req.message.trim();
    // Soportar prefijo [ISLAND:slug] enviado desde el selector de islas de AXIS UI
    let (mut selected_island_slug, msg) = if let Some(rest) = raw_msg.strip_prefix("[ISLAND:") {
        if let Some(end_b) = rest.find(']') {
            let slug = rest[..end_b].trim().to_string();
            let clean_m = rest[end_b + 1..].trim();
            (
                if slug.is_empty() || slug == "global" {
                    None
                } else {
                    Some(DynamicIslandManager::normalize_slug(&slug))
                },
                clean_m,
            )
        } else {
            (None, raw_msg)
        }
    } else {
        (None, raw_msg)
    };

    let mode = parsed_req.mode.unwrap_or_else(|| "normal".to_string());
    let now = now_ts();
    let mut tools_used = Vec::new();

    // 1. Vectorización del mensaje en CPU (768-dim)
    let t_emb = Instant::now();
    let query_vec = st.embedder.generate_vector(msg).unwrap_or_default();
    let ingest_ms = t_emb.elapsed().as_secs_f64() * 1000.0;

    // 2. Verificar si el usuario pidió crear una Isla de Vectores por texto o desde el botón "+ Nueva Isla"
    if let Some((isl_name, isl_desc)) = AgentToolkit::detect_island_creation_intent(msg) {
        let proto_vec = st
            .embedder
            .generate_vector(&format!("{} {}", isl_name, isl_desc))
            .unwrap_or_else(|_| query_vec.clone());
        let summary = st
            .custom_islands
            .create_island(&isl_name, &isl_desc, &proto_vec, now);
        selected_island_slug = Some(summary.slug.clone());
        tools_used.push(format!("CREATE_ISLAND ({})", summary.name));
    }

    // 3. Enrutamiento en Tálamo y Recuperación (Global vs Isla Personalizada 100% Aislada)
    let t_thal = Instant::now();
    let (routed_island, mut thal_sim) = st.thalamus.route_vector(&query_vec);
    let thalamus_us = t_thal.elapsed().as_secs_f64() * 1_000_000.0;

    let t_hippo = Instant::now();
    let (memory_context, seed_ids, causal_links, island_label) =
        if let Some(ref custom_slug) = selected_island_slug {
            if let Some((isolated_ctx, sim)) =
                st.custom_islands.recall_from_island(custom_slug, &query_vec, 6, now)
            {
                thal_sim = sim;
                (isolated_ctx, Vec::new(), Vec::new(), format!("Isla:{}", custom_slug))
            } else {
                // Si el usuario seleccionó un nombre de isla nuevo desde la UI, la inicializamos al vuelo
                let summary = st
                    .custom_islands
                    .create_island(custom_slug, msg, &query_vec, now);
                let (isolated_ctx, sim) = st
                    .custom_islands
                    .recall_from_island(&summary.slug, &query_vec, 6, now)
                    .unwrap_or_default();
                thal_sim = sim;
                (isolated_ctx, Vec::new(), Vec::new(), format!("Isla:{}", summary.name))
            }
        } else {
            let mem = st.hippocampus.recall(msg, query_vec.clone(), 5, now);
            let seeds = CausalHypergraph::extract_seed_ids(&mem);
            let links = st.causal_graph.propagate_from_seeds(&seeds, now);
            (mem, seeds, links, format!("{:?}", routed_island))
        };
    let hippocampus_us = t_hippo.elapsed().as_secs_f64() * 1_000_000.0;

    // Modos especiales (/deep-think, /quiron, /rem)
    if mode == "deep-think" || msg.starts_with("/deep-think") {
        let clean_problem = msg.trim_start_matches("/deep-think").trim();
        let mut tb = st.thought_buffer.lock().await;
        match BinaryScenarioTree::execute_deep_think(
            clean_problem,
            &memory_context,
            &st.cortex,
            &st.kv_manager,
            &st.friction_chamber,
            &st.gatekeeper,
            &mut tb,
            extract_rust_from_prefill,
        ) {
            Ok(win) => {
                let answer = format!(
                    "### [ABE /deep-think] Ganador: {} ({})\n- **Poda Sináptica:** `{}` descartado\n- **Tiempo de Compilación/Ejecución:** `{} µs` (Score: `{:.2}`)\n\n```rust\n{}\n```\n\n**STDOUT Verificado:**\n```text\n{}\n```",
                    win.winning_branch.name,
                    win.winning_branch.strategy,
                    win.pruned_branch_name,
                    win.winning_branch.eval_micros,
                    win.winning_branch.score,
                    win.winning_branch.code,
                    win.winning_branch.stdout.trim()
                );
                return ApiChatResponse {
                    ok: true,
                    answer,
                    thinking: tb.last_thought().map(|s| s.to_string()),
                    island: island_label,
                    active_custom_island: selected_island_slug,
                    custom_islands: st.custom_islands.list_islands(),
                    similarity: thal_sim,
                    ingest_ms,
                    thalamus_us,
                    hippocampus_us,
                    total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
                    causal_edges_total: st.causal_graph.total_edges(),
                    tools_used: vec!["rustc -O (ABE Dual Oracle)".to_string()],
                };
            }
            Err(e) => {
                return ApiChatResponse {
                    ok: false,
                    answer: format!("Error en ABE /deep-think:\n{}", e),
                    thinking: None,
                    island: island_label,
                    active_custom_island: selected_island_slug,
                    custom_islands: st.custom_islands.list_islands(),
                    similarity: thal_sim,
                    ingest_ms,
                    thalamus_us,
                    hippocampus_us,
                    total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
                    causal_edges_total: st.causal_graph.total_edges(),
                    tools_used: vec![],
                };
            }
        }
    }

    if mode == "quiron" || msg.starts_with("/quiron") {
        let clean_q = msg.trim_start_matches("/quiron").trim();
        let mut qs = st.quiron_shield.lock().await;
        let packet = qs.sanitize_context(clean_q, routed_island, TriggerReason::InvocacionExplicita);
        let rehydrated = qs.rehydrate(&packet.sanitized_code_or_query);
        let answer = format!(
            "### [Lego 5 — Escudo Quirón Zero-Knowledge]\n- **Tutor Seleccionado:** `{:?}`\n- **Símbolos Protegidos en RAM:** `{}`\n\n**Payload Ofuscado (Lo único que saldría a red):**\n```text\n{}\n```\n**Re-hidratación Verificada en Local:**\n```text\n{}\n```",
            packet.tutor, packet.symbol_table_size, packet.sanitized_code_or_query, rehydrated
        );
        return ApiChatResponse {
            ok: true,
            answer,
            thinking: None,
            island: island_label,
            active_custom_island: selected_island_slug,
            custom_islands: st.custom_islands.list_islands(),
            similarity: thal_sim,
            ingest_ms,
            thalamus_us,
            hippocampus_us,
            total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
            causal_edges_total: st.causal_graph.total_edges(),
            tools_used: vec!["QuironShield AST".to_string()],
        };
    }

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

    let current_scope_key = selected_island_slug
        .clone()
        .unwrap_or_else(|| "global".to_string());
    {
        let hist = get_recent_turns().lock().await;
        let scoped_turns: Vec<_> = hist
            .iter()
            .filter(|(scope, _, _)| *scope == current_scope_key)
            .collect();
        if !scoped_turns.is_empty() {
            full_context.push_str("\n[Historial Reciente de esta Conversación/Isla]:\n");
            for (i, (_, u_msg, b_ans)) in scoped_turns.iter().enumerate() {
                let short_ans: String = b_ans.chars().take(280).collect();
                full_context.push_str(&format!(
                    "Turno -{}: Daniel: \"{}\" -> Bob: \"{}...\"\n",
                    scoped_turns.len() - i,
                    u_msg,
                    short_ans
                ));
            }
        }
    }

    // Si estamos en una Isla Personalizada, no mezclamos los 26 repos globales a menos que se pida explícitamente
    if selected_island_slug.is_none() || msg.to_lowercase().contains("proyecto") || msg.to_lowercase().contains("repo") {
        if let Some(axis_ctx) = &parsed_req.axis_context {
            if !axis_ctx.trim().is_empty() {
                full_context.push_str(&format!("\n[Estado en Vivo de AXIS UI]:\n{}\n", axis_ctx));
            }
        }
    }

    // Reflejos Pre-Inferencia
    let mut reflex_calls = AgentToolkit::detect_reflex_tools(msg, thal_sim);
    if mode == "web"
        && !reflex_calls
            .iter()
            .any(|c| matches!(c, AgentToolCall::WebSearch { .. }))
    {
        reflex_calls.push(AgentToolCall::WebSearch {
            query: msg.to_string(),
        });
    }

    for call in &reflex_calls {
        if !matches!(call, AgentToolCall::CreateIsland { .. }) {
            let res = AgentToolkit::execute_tool_autonomous(call);
            tools_used.push(res.tool_name.to_string());
            full_context.push_str(&format!(
                "\n[OBSERVACIÓN EN VIVO DE {}]:\n{}\n",
                res.tool_name, res.output
            ));
        }
    }

    let island_directive = if let Some(ref slug) = selected_island_slug {
        format!(
            "MODO DE ISLA CORTICAL AISLADA ACTIVO (`{}`): Estás operando dentro de la isla de vectores `{}` en tu Hipocampo 4D. El 100% de lo que hables y analices en esta sesión queda guardado exclusivamente en esta isla especializada (separado del resto de proyectos).",
            slug, slug
        )
    } else {
        "Operando en el Hipocampo 4D Global (4 Islas Base + Hipergrafo Causal). Puedes crear nuevas islas especializadas en cualquier momento con `[TOOL:CREATE_ISLAND name=\"nombre\" desc=\"temas\"]`.".to_string()
    };

    let system_block = format!(
        "Eres Bob, el agente autónomo soberano de ingeniería de Daniel Garcia Malpica Benitez en Arch Linux (Hyprland), acoplado a AXIS.\n\
        {}\n\
        REGLA DE PENSAMIENTO: Tu bloque `<think>` debe ser menor a 10 líneas.\n\
        REGLA DE HERRAMIENTAS: Cuando uses `[TOOL:EXEC_CMD cmd=\"...\"]`, el servidor en Rust ejecutará tu comando bash/python inmediatamente en Arch Linux (con soporte completo para heredocs `cat << 'EOF'` y ventanas gráficas) y te mostrará el STDOUT/STDERR real.\n\
        {}",
        island_directive,
        AgentToolkit::system_prompt_instructions()
    );

    let mut rolling_user_context = format!(
        "Contexto Activo:\n{}\n\nInstrucción de Daniel: {}",
        full_context, msg
    );

    let mut final_clean_answer = String::new();
    let mut final_thinking: Option<String> = None;

    // Bucle Agéntico ReAct (si en cualquier paso emite [TOOL:...], SIEMPRE se ejecuta en el sistema)
    for step in 1..=3 {
        let prompt = format!(
            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            system_block, rolling_user_context
        );

        match st.cortex.generate(&prompt) {
            Ok(raw_response) => {
                let mut tb = st.thought_buffer.lock().await;
                let parsed = tb.process_output(&raw_response);
                if parsed.thinking_trace.is_some() {
                    final_thinking = parsed.thinking_trace.clone();
                }
                drop(tb);

                let llm_calls = AgentToolkit::parse_tool_calls(&parsed.clean_response);
                if llm_calls.is_empty() {
                    final_clean_answer = parsed.clean_response;
                    break;
                }

                let mut step_observations = String::new();
                for call in &llm_calls {
                    if let AgentToolCall::CreateIsland { name, description } = call {
                        let proto = st
                            .embedder
                            .generate_vector(&format!("{} {}", name, description))
                            .unwrap_or_else(|_| query_vec.clone());
                        let sum = st.custom_islands.create_island(name, description, &proto, now);
                        selected_island_slug = Some(sum.slug.clone());
                        tools_used.push(format!("CREATE_ISLAND:{}", sum.name));
                        step_observations.push_str(&format!(
                            "\n[ISLA CREADA EN HIPOCAMPO]: Isla `{}` (slug `{}`) activa con éxito.\n",
                            sum.name, sum.slug
                        ));
                    } else {
                        let res = AgentToolkit::execute_tool_autonomous(call);
                        tools_used.push(format!("{} (Paso {})", res.tool_name, step));
                        step_observations.push_str(&format!(
                            "\n[SALIDA REAL DE TERMINAL ARCH LINUX — {}]:\n{}\n",
                            res.tool_name, res.output
                        ));
                    }
                }

                // Si ya estamos en el paso 3 y emitió una herramienta, ya la ejecutamos arriba y adjuntamos la salida real
                if step == 3 {
                    final_clean_answer = format!(
                        "{}\n\n---\n### Resultado Real de Ejecución en Arch Linux\n```text\n{}\n```",
                        parsed.clean_response,
                        step_observations.trim()
                    );
                    break;
                }

                rolling_user_context.push_str(&format!(
                    "\n\n[Acción ejecutada en Paso {}]:\n{}\n\n[Resultado Real Obtenido en Arch Linux]:\n{}\n\nYa se ejecutó la herramienta anterior. Presenta ahora el reporte final a Daniel con la salida real obtenida sin volver a invocar herramientas.",
                    step, parsed.clean_response, step_observations
                ));
            }
            Err(e) => {
                return ApiChatResponse {
                    ok: false,
                    answer: format!("Error en Córtex: {}", e),
                    thinking: final_thinking,
                    island: island_label,
                    active_custom_island: selected_island_slug,
                    custom_islands: st.custom_islands.list_islands(),
                    similarity: thal_sim,
                    ingest_ms,
                    thalamus_us,
                    hippocampus_us,
                    total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
                    causal_edges_total: st.causal_graph.total_edges(),
                    tools_used,
                };
            }
        }
    }

    // Consolidación de Memoria (100% en la Isla Personalizada si está activa, o en el Hipocampo Global)
    if !final_clean_answer.trim().is_empty() {
        let final_scope = selected_island_slug
            .clone()
            .unwrap_or_else(|| "global".to_string());
        {
            let mut hist = get_recent_turns().lock().await;
            if hist.len() >= 18 {
                hist.remove(0);
            }
            hist.push((final_scope, msg.to_string(), final_clean_answer.clone()));
        }

        let mem_text = format!("Daniel: {} | Bob: {}", msg, final_clean_answer);
        if let Ok(mem_vec) = st.embedder.generate_vector(&mem_text) {
            if let Some(ref custom_slug) = selected_island_slug {
                st.custom_islands
                    .remember_in_island(custom_slug, mem_text, &mem_vec, now);
            } else {
                st.thalamus.reinforce_centroid(routed_island, &mem_vec, 0.05);
                let (nid, cat_saved) = st.hippocampus.remember(
                    mem_text.clone(),
                    mem_vec.clone(),
                    Some(routed_island),
                    1,
                    now,
                );
                st.custom_islands.register_base_node(
                    nid,
                    &format!("{:?}", cat_saved),
                    mem_text.clone(),
                    &mem_vec,
                    now,
                );
                if let Some(&primary_seed) = seed_ids.first() {
                    let short: String = mem_text.chars().take(110).collect();
                    st.causal_graph.link_nodes(
                        nid,
                        primary_seed,
                        CausalRelation::DependeDe,
                        short,
                        0.90,
                        now,
                    );
                }
            }
        }
    }

    for block in st.gatekeeper.scan_executable_blocks(&final_clean_answer) {
        if block.lang == CodeLang::Rust
            && !block.code.contains("use eframe")
            && !block.code.contains("use egui")
            && !block.code.contains("use serde")
            && !block.code.contains("use tokio")
            && !block.code.contains("use iced")
        {
            let verify = st.friction_chamber.verify_rust_code(&block.code, 1);
            tools_used.push(format!("Oráculo rustc std (exit={})", verify.exit_code));
        }
    }

    ApiChatResponse {
        ok: true,
        answer: final_clean_answer,
        thinking: final_thinking,
        island: island_label,
        active_custom_island: selected_island_slug,
        custom_islands: st.custom_islands.list_islands(),
        similarity: thal_sim,
        ingest_ms,
        thalamus_us,
        hippocampus_us,
        total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
        causal_edges_total: st.causal_graph.total_edges(),
        tools_used,
    }
}
