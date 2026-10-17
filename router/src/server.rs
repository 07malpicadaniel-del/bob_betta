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
use vector_db::causal_plane::{CausalHypergraph, CausalRelation, VectorThalamus};
use vector_db::common::types::IslandCategory;
use vector_db::ingestion::embedder::NativeEmbedder;
use vector_db::HippocampusEngine;

use crate::abe::BinaryScenarioTree;
use crate::ecosystem_seed::EcosystemSeeder;
use crate::extract_rust_from_prefill;
use crate::gatekeeper::{CodeLang, Gatekeeper};
use crate::quiron::{QuironShield, TriggerReason};
use crate::thought_buffer::ThoughtBuffer;
use crate::tools::AgentToolkit;

pub struct BobSharedState {
    pub hippocampus: Arc<HippocampusEngine>,
    pub embedder: Arc<NativeEmbedder>,
    pub thalamus: Arc<VectorThalamus>,
    pub causal_graph: Arc<CausalHypergraph>,
    pub cortex: Arc<CortexEngine>,
    pub kv_manager: Arc<KvCacheManager>,
    pub friction_chamber: Arc<FrictionChamber>,
    pub gatekeeper: Arc<Gatekeeper>,
    pub thought_buffer: Mutex<ThoughtBuffer>,
    pub quiron_shield: Mutex<QuironShield>,
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
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub async fn start_axis_bridge_server(state: Arc<BobSharedState>, port: u16) {
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
                let mut buf = vec![0u8; 65536];
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

                let json_string = if first_line.starts_with("GET /status") {
                    let status = ApiStatusResponse {
                        online: true,
                        model: "Qwen3.8-27B-Uncensored-IQ4_XS (99 capas RTX 5060 Ti)".to_string(),
                        causal_edges: st.causal_graph.total_edges(),
                        sys_info: AgentToolkit::execute_tool(&crate::tools::AgentToolCall::SysInfo).output,
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
                        "causal_edges": st.causal_graph.total_edges()
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

    let msg = parsed_req.message.trim();
    let mode = parsed_req.mode.unwrap_or_else(|| "normal".to_string());
    let now = now_ts();

    // 1. Ingesta y Enrutamiento Tálamo SIMD
    let t_emb = Instant::now();
    let query_vec = st.embedder.generate_vector(msg).unwrap_or_default();
    let ingest_ms = t_emb.elapsed().as_secs_f64() * 1000.0;

    let t_thal = Instant::now();
    let (routed_island, thal_sim) = st.thalamus.route_vector(&query_vec);
    let thalamus_us = t_thal.elapsed().as_secs_f64() * 1_000_000.0;

    let t_hippo = Instant::now();
    let memory_context = st.hippocampus.recall(msg, query_vec.clone(), 4, now);
    let seed_ids = CausalHypergraph::extract_seed_ids(&memory_context);
    let causal_links = st.causal_graph.propagate_from_seeds(&seed_ids, now);
    let hippocampus_us = t_hippo.elapsed().as_secs_f64() * 1_000_000.0;

    // Modos especiales invocados desde los botones de AXIS UI
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
                    island: format!("{:?}", IslandCategory::Code),
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
                    island: format!("{:?}", routed_island),
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
            island: format!("{:?}", routed_island),
            similarity: thal_sim,
            ingest_ms,
            thalamus_us,
            hippocampus_us,
            total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
            causal_edges_total: st.causal_graph.total_edges(),
            tools_used: vec!["QuironShield AST".to_string()],
        };
    }

    if mode == "rem" || msg == "/rem" {
        let probe_a = CausalHypergraph::random_hypersphere_probe();
        let probe_b = CausalHypergraph::random_hypersphere_probe();
        let raw_a = st.hippocampus.recall("architecture axiom leika axis", probe_a, 3, now);
        let raw_b = st.hippocampus.recall("rust code algorithm simd", probe_b, 3, now);
        let mut candidates = CausalHypergraph::extract_nodes_with_text(&raw_a);
        for (id, txt) in CausalHypergraph::extract_nodes_with_text(&raw_b) {
            if !candidates.iter().any(|(cid, _)| *cid == id) {
                candidates.push((id, txt));
            }
        }
        let (pa_id, ca) = candidates.first().cloned().unwrap_or((0, "Hipocampo 4D en 768 dims".to_string()));
        let (pb_id, cb) = candidates.get(1).cloned().unwrap_or((1, "Orquestador Axis Tauri".to_string()));
        let short_a: String = ca.chars().take(100).collect();
        let short_b: String = cb.chars().take(100).collect();

        let rem_prompt = FrictionChamber::build_rem_dream_prompt(&short_a, &short_b);
        if let Ok(raw_rem) = st.cortex.generate(&rem_prompt) {
            let mut tb = st.thought_buffer.lock().await;
            let parsed_rem = tb.process_output(&raw_rem);
            if let Some(code) = extract_rust_from_prefill(&parsed_rem.clean_response, &st.gatekeeper) {
                let ver = st.friction_chamber.verify_rust_code(&code, 1);
                if ver.compiled_ok {
                    st.causal_graph.link_nodes(pa_id, pb_id, CausalRelation::Optimiza, "Sueño REM validado desde Axis".to_string(), 0.95, now);
                    let answer = format!(
                        "### [Ciclo Hyper-REM Completado]\n- **Nodo Padre #{}:** `{}`\n- **Nodo Padre #{}:** `{}`\n\n```rust\n{}\n```\n**STDOUT (`exit_code=0`):**\n```text\n{}\n```",
                        pa_id, short_a, pb_id, short_b, code, ver.stdout.trim()
                    );
                    return ApiChatResponse {
                        ok: true,
                        answer,
                        thinking: parsed_rem.thinking_trace,
                        island: "Code".to_string(),
                        similarity: thal_sim,
                        ingest_ms,
                        thalamus_us,
                        hippocampus_us,
                        total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
                        causal_edges_total: st.causal_graph.total_edges(),
                        tools_used: vec!["FrictionChamber rustc".to_string()],
                    };
                }
            }
        }
    }

    // Flujo Conversacional y Agéntico ReAct con Contexto de Axis
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

    if let Some(axis_ctx) = &parsed_req.axis_context {
        if !axis_ctx.trim().is_empty() {
            full_context.push_str(&format!("\n[Estado en Vivo de AXIS UI]:\n{}\n", axis_ctx));
        }
    }

    let mut tools_used = Vec::new();
    let mut reflex_calls = AgentToolkit::detect_reflex_tools(msg, thal_sim);
    if mode == "web" && !reflex_calls.iter().any(|c| matches!(c, crate::tools::AgentToolCall::WebSearch { .. })) {
        reflex_calls.push(crate::tools::AgentToolCall::WebSearch { query: msg.to_string() });
    }

    for call in &reflex_calls {
        // Desde la UI solo auto-ejecutamos herramientas de lectura/inspección/web sin bloqueo de stdin
        if !matches!(call, crate::tools::AgentToolCall::ExecCmd { .. } | crate::tools::AgentToolCall::WriteFile { .. }) {
            let res = AgentToolkit::execute_tool(call);
            tools_used.push(res.tool_name.to_string());
            full_context.push_str(&format!("\n[OBSERVACIÓN EN VIVO DE {}]:\n{}\n", res.tool_name, res.output));
        }
    }

    let system_block = format!(
        "Eres Bob, el agente autónomo central de ingeniería de Daniel Garcia Malpica Benitez, integrado dentro de su plataforma AXIS en Arch Linux.\n\
        Conoces todos los proyectos de Daniel (LEIKA, AXIS, AURORA, Sertra, Molex, ObraVoice, NanoTechLab, The Outpost, vector_DB_ll, beta_brain67) y su hardware (Ryzen 9 7900, 64 GB RAM, RTX 5060 Ti 16 GB).\n\
        Responde siempre en español de forma técnica, estructurada y precisa.\n\
        {}",
        AgentToolkit::system_prompt_instructions()
    );

    let final_prompt = format!(
        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\nContexto del Hipocampo 4D y Telemetría:\n{}\n\nMensaje de Daniel desde AXIS UI: {}<|im_end|>\n<|im_start|>assistant\n",
        system_block, full_context, msg
    );

    match st.cortex.generate(&final_prompt) {
        Ok(raw_response) => {
            let mut tb = st.thought_buffer.lock().await;
            let parsed = tb.process_output(&raw_response);

            if !parsed.clean_response.trim().is_empty() {
                let mem_text = format!("Usuario (AXIS): {} | Bob: {}", msg, parsed.clean_response);
                if let Ok(mem_vec) = st.embedder.generate_vector(&mem_text) {
                    st.thalamus.reinforce_centroid(routed_island, &mem_vec, 0.05);
                    let (nid, _) = st.hippocampus.remember(
                        mem_text.clone(),
                        mem_vec,
                        Some(routed_island),
                        1,
                        now,
                    );
                    if let Some(&primary_seed) = seed_ids.first() {
                        let short: String = mem_text.chars().take(110).collect();
                        st.causal_graph.link_nodes(nid, primary_seed, CausalRelation::DependeDe, short, 0.90, now);
                    }
                }
            }

            // Si el código generado contiene un snippet Rust con fn main, lo validamos en la Cámara de Fricción
            for block in st.gatekeeper.scan_executable_blocks(&parsed.clean_response) {
                if block.lang == CodeLang::Rust {
                    let verify = st.friction_chamber.verify_rust_code(&block.code, 1);
                    tools_used.push(format!("Oráculo rustc (exit={})", verify.exit_code));
                }
            }

            ApiChatResponse {
                ok: true,
                answer: parsed.clean_response,
                thinking: parsed.thinking_trace,
                island: format!("{:?}", routed_island),
                similarity: thal_sim,
                ingest_ms,
                thalamus_us,
                hippocampus_us,
                total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
                causal_edges_total: st.causal_graph.total_edges(),
                tools_used,
            }
        }
        Err(e) => ApiChatResponse {
            ok: false,
            answer: format!("Error en Córtex: {}", e),
            thinking: None,
            island: format!("{:?}", routed_island),
            similarity: thal_sim,
            ingest_ms,
            thalamus_us,
            hippocampus_us,
            total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
            causal_edges_total: st.causal_graph.total_edges(),
            tools_used,
        },
    }
}
