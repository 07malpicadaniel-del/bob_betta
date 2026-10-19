use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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
use vector_db::common::types::IslandCategory;
use vector_db::ingestion::embedder::NativeEmbedder;
use vector_db::HippocampusEngine;

use crate::abe::BinaryScenarioTree;
use crate::ecosystem_seed::EcosystemSeeder;
use crate::extract_rust_from_prefill;
use crate::gatekeeper::{CodeLang, Gatekeeper};
use crate::marathon::{get_marathon_state, MarathonEngine};
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

static VOICE_RECORDER: std::sync::OnceLock<Mutex<Option<Child>>> = std::sync::OnceLock::new();

fn get_voice_recorder() -> &'static Mutex<Option<Child>> {
    VOICE_RECORDER.get_or_init(|| Mutex::new(None))
}

#[derive(Deserialize)]
#[serde(crate = "cortex::serde")]
struct SpeakReq {
    text: String,
    lang: Option<String>,
}

#[derive(Deserialize)]
#[serde(crate = "cortex::serde")]
struct VoiceStopReq {
    lang: Option<String>,
}

#[derive(Deserialize)]
#[serde(crate = "cortex::serde")]
struct MarathonStartReq {
    goal: String,
    max_cycles: Option<usize>,
}

#[derive(Deserialize)]
#[serde(crate = "cortex::serde")]
struct WorkspaceInitReq {
    session_id: String,
    title: Option<String>,
    island: Option<String>,
}

#[derive(Deserialize)]
#[serde(crate = "cortex::serde")]
struct FileUploadReq {
    session_id: String,
    island: Option<String>,
    filename: String,
    content_base64: String,
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
    workspace_path: String,
    workspace_files: Vec<String>,
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

fn sanitize_session_folder(session_id: &str) -> String {
    let cleaned: String = session_id
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "default_session".to_string()
    } else {
        cleaned
    }
}

/// Crea o recupera la carpeta de trabajo dedicada a una conversación y la indexa en el Hipocampo 4D
fn ensure_session_workspace(
    st: &Arc<BobSharedState>,
    session_id: &str,
    title: Option<&str>,
    island_slug: Option<&str>,
) -> (PathBuf, Vec<String>) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
    let folder_name = sanitize_session_folder(session_id);
    let ws_dir = PathBuf::from(&home)
        .join("proyectos")
        .join("bob_workspaces")
        .join(&folder_name);

    let is_new = !ws_dir.exists();
    let _ = fs::create_dir_all(ws_dir.join("src"));
    let _ = fs::create_dir_all(ws_dir.join("attachments"));

    let manifest_path = ws_dir.join("WORKSPACE_MANIFEST.md");
    if !manifest_path.exists() {
        let initial_manifest = format!(
            "# Workspace de Sesión BOB: {}\n\
            - **ID de Sesión:** `{}`\n\
            - **Ruta en Disco:** `{}`\n\
            - **Isla Vinculada:** `{}`\n\n\
            ## Arquitectura y Archivos Generados\n\
            - `src/`: Código fuente del proyecto de esta sesión.\n\
            - `attachments/`: Imágenes y archivos adjuntados por Daniel.\n\
            - `ERROR_LOG.md`: Registro de errores de compilación/ejecución y soluciones aplicadas.\n",
            title.unwrap_or("Proyecto en curso"),
            session_id,
            ws_dir.display(),
            island_slug.unwrap_or("global")
        );
        let _ = fs::write(&manifest_path, initial_manifest);
    }

    let err_log_path = ws_dir.join("ERROR_LOG.md");
    if !err_log_path.exists() {
        let initial_err = "# Bitácora de Errores y Auto-Corrección (BOB)\n\
        Aquí documento cada fallo de compilación, dependencia o lógica detectado en esta sesión para no repetirlo.\n";
        let _ = fs::write(&err_log_path, initial_err);
    }

    if is_new {
        let now = now_ts();
        let ws_note = format!(
            "Workspace de Proyecto Inicializado [{}]: Carpeta `{}` creada para almacenar código, manifiesto arquitectónico y bitácora de errores de la conversación `{}`.",
            folder_name,
            ws_dir.display(),
            title.unwrap_or(&folder_name)
        );
        if let Ok(vec) = st.embedder.generate_vector(&ws_note) {
            if let Some(isl) = island_slug.filter(|s| !s.is_empty() && *s != "global") {
                st.custom_islands.remember_in_island(isl, ws_note, &vec, now);
            } else {
                let (nid, _) = st.hippocampus.remember(
                    ws_note.clone(),
                    vec.clone(),
                    Some(IslandCategory::WorkingMemory),
                    1,
                    now,
                );
                st.custom_islands.register_base_node(
                    nid,
                    "WorkingMemory",
                    ws_note,
                    &vec,
                    now,
                );
            }
        }
    }

    let files = list_workspace_files_recursive(&ws_dir, &ws_dir);
    (ws_dir, files)
}

fn list_workspace_files_recursive(base: &Path, current: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(current) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "target" || name == "node_modules" || name.starts_with(".git") {
            continue;
        }
        if p.is_dir() {
            out.extend(list_workspace_files_recursive(base, &p));
        } else if let Ok(rel) = p.strip_prefix(base) {
            out.push(rel.to_string_lossy().to_string());
        }
    }
    out.sort();
    out
}

/// Construye un resumen vivo de la carpeta del proyecto para inyectar en el prompt sin saturar la ventana de tokens
fn build_workspace_live_snapshot(ws_dir: &Path, files: &[String]) -> String {
    let mut snap = format!(
        "### [MEMORIA EXTERNA DE PROYECTO EN DISCO (WORKSPACE DE ESTA CONVERSACIÓN)]\n\
        - **Carpeta Raíz de este Chat:** `{}`\n\
        - **Archivos actuales en la carpeta ({}):** {:?}\n",
        ws_dir.display(),
        files.len(),
        files
    );

    let manifest_p = ws_dir.join("WORKSPACE_MANIFEST.md");
    if let Ok(m_txt) = fs::read_to_string(&manifest_p) {
        let short_m: String = m_txt.chars().take(900).collect();
        snap.push_str(&format!("\n**[WORKSPACE_MANIFEST.md]:**\n{}\n", short_m.trim()));
    }

    let err_p = ws_dir.join("ERROR_LOG.md");
    if let Ok(e_txt) = fs::read_to_string(&err_p) {
        let short_e: String = e_txt.chars().rev().take(700).collect::<String>().chars().rev().collect();
        snap.push_str(&format!("\n**[ERROR_LOG.md (Últimos registros)]:**\n{}\n", short_e.trim()));
    }

    snap
}

fn decode_base64_via_python(b64: &str, dest_path: &Path) -> Result<usize, String> {
    let tmp_b64 = "/tmp/bob_upload_payload.b64";
    fs::write(tmp_b64, b64).map_err(|e| e.to_string())?;
    let py_code = format!(
        "import base64; data = open('{}', 'rb').read(); raw = base64.b64decode(data); open('{}', 'wb').write(raw); print(len(raw))",
        tmp_b64,
        dest_path.display()
    );
    let out = Command::new("python3")
        .args(["-c", &py_code])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        let len_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(len_str.parse::<usize>().unwrap_or(0))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
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
                // Leer petición completa (hasta 15 MB para soportar imágenes y archivos en Base64)
                let mut buf = Vec::with_capacity(65536);
                let mut temp = [0u8; 65536];
                let mut expected_total_len: Option<usize> = None;

                loop {
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(1200),
                        stream.read(&mut temp),
                    )
                    .await
                    {
                        Ok(Ok(0)) => break,
                        Ok(Ok(n)) => {
                            buf.extend_from_slice(&temp[..n]);
                            if expected_total_len.is_none() {
                                let probe = String::from_utf8_lossy(&buf);
                                if let Some(hdr_end) = probe.find("\r\n\r\n") {
                                    let headers = &probe[..hdr_end];
                                    let mut content_len = 0usize;
                                    for line in headers.lines() {
                                        if line.to_lowercase().starts_with("content-length:") {
                                            if let Some(val) = line.split(':').nth(1) {
                                                content_len = val.trim().parse::<usize>().unwrap_or(0);
                                            }
                                        }
                                    }
                                    expected_total_len = Some(hdr_end + 4 + content_len);
                                }
                            }
                            if let Some(exp) = expected_total_len {
                                if buf.len() >= exp {
                                    break;
                                }
                            }
                        }
                        _ => break,
                    }
                }

                if buf.is_empty() {
                    return;
                }

                let req_str = String::from_utf8_lossy(&buf);
                let first_line = req_str.lines().next().unwrap_or("");
                let body_start = req_str.find("\r\n\r\n").map(|i| i + 4).unwrap_or(buf.len());
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
                } else if first_line.starts_with("POST /session/workspace") {
                    if let Ok(req) = serde_json::from_str::<WorkspaceInitReq>(body) {
                        let (ws_path, files) = ensure_session_workspace(
                            &st,
                            &req.session_id,
                            req.title.as_deref(),
                            req.island.as_deref(),
                        );
                        serde_json::json!({
                            "ok": true,
                            "workspace_path": ws_path.to_string_lossy(),
                            "files": files
                        })
                        .to_string()
                    } else {
                        serde_json::json!({"ok": false, "error": "JSON invalido"}).to_string()
                    }
                } else if first_line.starts_with("POST /session/paste") || first_line.starts_with("POST /session/pick") {
                    let mode_arg = if first_line.starts_with("POST /session/paste") { "paste" } else { "pick" };
                    if let Ok(req) = serde_json::from_str::<WorkspaceInitReq>(body) {
                        let (ws_path, _) = ensure_session_workspace(
                            &st,
                            &req.session_id,
                            req.title.as_deref(),
                            req.island.as_deref(),
                        );
                        let attach_dir = ws_path.join("attachments");
                        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
                        let py_bin = format!("{}/proyectos/bob_engine/.venv/bin/python3", home);
                        let script = format!("{}/proyectos/bob_engine/scripts/bob_clipboard_and_picker.py", home);
                        let out = Command::new(&py_bin)
                            .arg(&script)
                            .arg(mode_arg)
                            .arg(&attach_dir)
                            .output();

                        match out {
                            Ok(o) => {
                                let stdout = String::from_utf8_lossy(&o.stdout);
                                if let Some(jline) = stdout.lines().find(|l| l.starts_with("JSON_OUT:")) {
                                    let raw_json = jline.trim_start_matches("JSON_OUT:").trim();
                                    if let Ok(mut parsed_val) = serde_json::from_str::<serde_json::Value>(raw_json) {
                                        if parsed_val["ok"].as_bool() == Some(true) {
                                            if let Some(items) = parsed_val["items"].as_array() {
                                                let now = now_ts();
                                                for item in items {
                                                    if let Some(sum_txt) = item["summary"].as_str() {
                                                        if let Ok(vec) = st.embedder.generate_vector(sum_txt) {
                                                            if let Some(isl) = req.island.as_deref().filter(|s| !s.is_empty() && *s != "global") {
                                                                st.custom_islands.remember_in_island(isl, sum_txt.to_string(), &vec, now);
                                                            } else {
                                                                let (nid, _) = st.hippocampus.remember(sum_txt.to_string(), vec.clone(), Some(IslandCategory::Code), 1, now);
                                                                st.custom_islands.register_base_node(nid, "Code", sum_txt.to_string(), &vec, now);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        let updated_files = list_workspace_files_recursive(&ws_path, &ws_path);
                                        parsed_val["workspace_files"] = serde_json::json!(updated_files);
                                        parsed_val.to_string()
                                    } else {
                                        serde_json::json!({"ok": false, "error": "Error parseando salida de clipboard"}).to_string()
                                    }
                                } else {
                                    serde_json::json!({"ok": false, "error": "No se obtuvo respuesta del helper"}).to_string()
                                }
                            }
                            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
                        }
                    } else {
                        serde_json::json!({"ok": false, "error": "JSON invalido"}).to_string()
                    }
                } else if first_line.starts_with("POST /session/upload") {
                    if let Ok(req) = serde_json::from_str::<FileUploadReq>(body) {
                        let (ws_path, _) = ensure_session_workspace(
                            &st,
                            &req.session_id,
                            None,
                            req.island.as_deref(),
                        );
                        let safe_fname: String = req
                            .filename
                            .chars()
                            .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
                            .collect();
                        let dest = ws_path.join("attachments").join(&safe_fname);

                        match decode_base64_via_python(&req.content_base64, &dest) {
                            Ok(byte_len) => {
                                let ext = dest
                                    .extension()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("")
                                    .to_lowercase();
                                let is_img = matches!(
                                    ext.as_str(),
                                    "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif"
                                );

                                let analysis_summary = if is_img {
                                    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
                                    let py_bin = format!("{}/proyectos/bob_engine/.venv/bin/python3", home);
                                    let script = format!("{}/proyectos/bob_engine/scripts/bob_vision_ingest.py", home);
                                    let out = Command::new(&py_bin)
                                        .arg(&script).arg(&dest)
                                        .output();
                                    if let Ok(o) = out {
                                        let stdout = String::from_utf8_lossy(&o.stdout);
                                        if let Some(jline) = stdout.lines().find(|l| l.starts_with("JSON_OUT:")) {
                                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(jline.trim_start_matches("JSON_OUT:")) {
                                                val["summary"].as_str().unwrap_or("Imagen guardada.").to_string()
                                            } else {
                                                format!("Imagen guardada en {}", dest.display())
                                            }
                                        } else {
                                            format!("Imagen guardada en {}", dest.display())
                                        }
                                    } else {
                                        format!("Imagen guardada en {}", dest.display())
                                    }
                                } else {
                                    let raw_txt = fs::read_to_string(&dest).unwrap_or_default();
                                    let preview: String = raw_txt.chars().take(2500).collect();
                                    format!(
                                        "[ARCHIVO ADJUNTO EN WORKSPACE: `{}` ({} bytes)]\nRuta completa: `{}`\nContenido:\n```\n{}\n```",
                                        safe_fname,
                                        byte_len,
                                        dest.display(),
                                        preview
                                    )
                                };

                                // Indexar el archivo o imagen en el Hipocampo 4D
                                let now = now_ts();
                                if let Ok(vec) = st.embedder.generate_vector(&analysis_summary) {
                                    if let Some(isl) = req.island.as_deref().filter(|s| !s.is_empty() && *s != "global") {
                                        st.custom_islands.remember_in_island(isl, analysis_summary.clone(), &vec, now);
                                    } else {
                                        let (nid, _) = st.hippocampus.remember(
                                            analysis_summary.clone(),
                                            vec.clone(),
                                            Some(IslandCategory::Code),
                                            1,
                                            now,
                                        );
                                        st.custom_islands.register_base_node(
                                            nid,
                                            "Code",
                                            analysis_summary.clone(),
                                            &vec,
                                            now,
                                        );
                                    }
                                }

                                let updated_files = list_workspace_files_recursive(&ws_path, &ws_path);
                                serde_json::json!({
                                    "ok": true,
                                    "filename": safe_fname,
                                    "saved_path": dest.to_string_lossy(),
                                    "is_image": is_img,
                                    "summary": analysis_summary,
                                    "workspace_files": updated_files
                                })
                                .to_string()
                            }
                            Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
                        }
                    } else {
                        serde_json::json!({"ok": false, "error": "JSON de archivo invalido"}).to_string()
                    }
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
                } else if first_line.starts_with("POST /voice/start") {
                    let mut guard = get_voice_recorder().lock().await;
                    if let Some(mut prev) = guard.take() {
                        let _ = prev.kill();
                    }
                    let _ = std::fs::remove_file("/tmp/bob_voice_in.wav");
                    let child = Command::new("pw-record")
                        .args(["--rate", "16000", "--channels", "1", "--format", "s16", "/tmp/bob_voice_in.wav"])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .or_else(|_| {
                            Command::new("arecord")
                                .args(["-f", "S16_LE", "-r", "16000", "-c", "1", "/tmp/bob_voice_in.wav"])
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .spawn()
                        });
                    match child {
                        Ok(c) => {
                            *guard = Some(c);
                            serde_json::json!({"ok": true, "status": "recording"}).to_string()
                        }
                        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
                    }
                } else if first_line.starts_with("POST /voice/stop") {
                    {
                        let mut guard = get_voice_recorder().lock().await;
                        if let Some(mut child) = guard.take() {
                            let pid = child.id().to_string();
                            let _ = Command::new("kill").args(["-s", "INT", &pid]).status();
                            tokio::time::sleep(std::time::Duration::from_millis(280)).await;
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                    }
                    let req_lang = serde_json::from_str::<VoiceStopReq>(body)
                        .ok()
                        .and_then(|r| r.lang)
                        .unwrap_or_else(|| "auto".to_string());

                    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
                    let py_bin = format!("{}/proyectos/bob_engine/.venv/bin/python3", home);
                    let script = format!("{}/proyectos/bob_engine/scripts/bob_whisper_stt.py", home);
                    let out = Command::new(&py_bin)
                        .args([&script, "/tmp/bob_voice_in.wav", &req_lang])
                        .output();
                    match out {
                        Ok(o) => {
                            let stdout = String::from_utf8_lossy(&o.stdout);
                            if let Some(json_line) = stdout.lines().find(|l| l.starts_with("JSON_OUT:")) {
                                json_line.trim_start_matches("JSON_OUT:").trim().to_string()
                            } else {
                                serde_json::json!({"ok": false, "error": "No se pudo procesar el audio"}).to_string()
                            }
                        }
                        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
                    }
                } else if first_line.starts_with("POST /voice/speak") {
                    if let Ok(req) = serde_json::from_str::<SpeakReq>(body) {
                        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
                        let py_bin = format!("{}/proyectos/bob_engine/.venv/bin/python3", home);
                        let script = format!("{}/proyectos/bob_engine/scripts/bob_tts_speak.py", home);
                        let lang = req.lang.unwrap_or_else(|| "es".to_string());
                        let _ = Command::new("pkill").args(["-f", "bob_tts_out.mp3"]).status();
                        let _ = Command::new(&py_bin).args([&script, &req.text, &lang]).spawn();
                        serde_json::json!({"ok": true}).to_string()
                    } else {
                        serde_json::json!({"ok": false}).to_string()
                    }
                } else if first_line.starts_with("POST /browser/launch") {
                    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
                    let profile_dir = format!("{}/.config/bob_browser_profile", home);
                    let launch_cmd = format!(
                        "(google-chrome-stable --remote-debugging-port=9222 --user-data-dir='{}' https://gemini.google.com/app || brave --remote-debugging-port=9222 --user-data-dir='{}' https://gemini.google.com/app || chromium --remote-debugging-port=9222 --user-data-dir='{}' https://gemini.google.com/app) >/dev/null 2>&1 &",
                        profile_dir, profile_dir, profile_dir
                    );
                    let _ = Command::new("bash").arg("-c").arg(&launch_cmd).spawn();
                    serde_json::json!({
                        "ok": true,
                        "msg": "Navegador Puente lanzado en puerto 9222. Inicia sesion en tu cuenta si es la primera vez."
                    })
                    .to_string()
                } else if first_line.starts_with("POST /marathon/start") {
                    if let Ok(req) = serde_json::from_str::<MarathonStartReq>(body) {
                        let cycles = req.max_cycles.unwrap_or(30);
                        MarathonEngine::start_marathon(Arc::clone(&st), req.goal, cycles).await;
                        serde_json::json!({"ok": true, "status": "started"}).to_string()
                    } else {
                        serde_json::json!({"ok": false, "error": "JSON invalido"}).to_string()
                    }
                } else if first_line.starts_with("GET /marathon/status") {
                    let m = get_marathon_state().lock().await;
                    serde_json::to_string(&*m).unwrap_or_else(|_| "{}".to_string())
                } else if first_line.starts_with("POST /marathon/stop") {
                    MarathonEngine::stop_marathon().await;
                    serde_json::json!({"ok": true, "status": "stopped"}).to_string()
                } else if first_line.starts_with("POST /chat") {
                    let resp_struct = handle_axis_chat(&st, body).await;
                    serde_json::to_string(&resp_struct).unwrap_or_else(|_| "{}".to_string())
                } else {
                    serde_json::json!({"error": "endpoint desconocido"}).to_string()
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
                workspace_path: String::new(),
                workspace_files: vec![],
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

    let raw_input = parsed_req.message.trim();
    let (session_id, raw_msg) = if let Some(rest) = raw_input.strip_prefix("[SESSION:") {
        if let Some(end_s) = rest.find(']') {
            (rest[..end_s].trim().to_string(), rest[end_s + 1..].trim())
        } else {
            ("default".to_string(), raw_input)
        }
    } else {
        ("default".to_string(), raw_input)
    };

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

    // Garantizar que exista el Workspace en disco para esta sesión de chat y leer sus archivos actuales
    let (ws_dir, mut ws_files) = ensure_session_workspace(
        st,
        &session_id,
        None,
        selected_island_slug.as_deref(),
    );
    let ws_path_str = ws_dir.to_string_lossy().to_string();

    let mode = parsed_req.mode.unwrap_or_else(|| "normal".to_string());
    let now = now_ts();
    let mut tools_used = Vec::new();

    let t_emb = Instant::now();
    let query_vec = st.embedder.generate_vector(msg).unwrap_or_default();
    let ingest_ms = t_emb.elapsed().as_secs_f64() * 1000.0;

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
                    workspace_path: ws_path_str,
                    workspace_files: ws_files,
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
                    workspace_path: ws_path_str,
                    workspace_files: ws_files,
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
            workspace_path: ws_path_str,
            workspace_files: ws_files,
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

    // Inyectar siempre el estado en vivo del Workspace en disco de esta conversación
    full_context.push_str("\n");
    full_context.push_str(&build_workspace_live_snapshot(&ws_dir, &ws_files));

    let current_scope_key = format!(
        "{}::{}",
        session_id,
        selected_island_slug.clone().unwrap_or_else(|| "global".to_string())
    );
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

    if selected_island_slug.is_none()
        || msg.to_lowercase().contains("proyecto")
        || msg.to_lowercase().contains("repo")
    {
        if let Some(axis_ctx) = &parsed_req.axis_context {
            if !axis_ctx.trim().is_empty() {
                full_context.push_str(&format!("\n[Estado en Vivo de AXIS UI]:\n{}\n", axis_ctx));
            }
        }
    }

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
            "MODO DE ISLA CORTICAL AISLADA ACTIVO (`{}`): Estás operando dentro de la isla de vectores `{}` en tu Hipocampo 4D.",
            slug, slug
        )
    } else {
        "Operando en el Hipocampo 4D Global (4 Islas Base + Hipergrafo Causal).".to_string()
    };

    let system_block = format!(
        "Eres Bob, el agente autónomo soberano de ingeniería de Daniel Garcia Malpica Benitez en Arch Linux (Hyprland), acoplado a AXIS.\n\
        {}\n\
        CARPETA DE PROYECTO DE ESTA CONVERSACIÓN (MEMORIA EXTERNA CONTRA LÍMITE DE CONTEXTO): Tienes asignada la carpeta exclusiva `{}` en disco para este chat.\n\
        - Cuando construyas un proyecto grande, guarda su código dentro de `{}/src/` (o en el repositorio local que Daniel te indique), actualiza `{}/WORKSPACE_MANIFEST.md` con la estructura de archivos y registra cualquier error de compilación y su solución en `{}/ERROR_LOG.md`.\n\
        - Puedes releer cualquier archivo generado usando `[TOOL:READ_FILE path=\"...\"]` en cualquier momento para no depender de la ventana corta de tokens.\n\
        REGLA DE PENSAMIENTO: Tu bloque `<think>` debe ser menor a 10 líneas.\n\
        {}",
        island_directive,
        ws_dir.display(),
        ws_dir.display(),
        ws_dir.display(),
        ws_dir.display(),
        AgentToolkit::system_prompt_instructions()
    );

    let safe_context: String = if full_context.chars().count() > 7500 {
        full_context.chars().take(7500).collect::<String>()
            + "\n...[Contexto truncado para proteger ventana VRAM]..."
    } else {
        full_context
    };

    let mut rolling_user_context = format!(
        "Contexto Activo:\n{}\n\nInstrucción de Daniel: {}",
        safe_context, msg
    );

    let mut final_clean_answer = String::new();
    let mut final_thinking: Option<String> = None;

    for step in 1..=12 {
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

                        // Si hubo un error de compilación/ejecución, documentarlo automáticamente en ERROR_LOG.md de la sesión
                        if res.output.contains("Exit Code: 1")
                            || res.output.contains("error[")
                            || res.output.contains("Traceback")
                            || res.output.contains("SyntaxError")
                        {
                            let err_entry = format!(
                                "\n## [Ts: {}] Error detectado en {}\n```text\n{}\n```\n",
                                now,
                                res.tool_name,
                                res.output.chars().take(500).collect::<String>()
                            );
                            if let Ok(mut existing) = fs::read_to_string(ws_dir.join("ERROR_LOG.md")) {
                                existing.push_str(&err_entry);
                                let _ = fs::write(ws_dir.join("ERROR_LOG.md"), existing);
                            }
                        }
                    }
                }

                if step == 12 {
                    final_clean_answer = format!(
                        "{}\n\n---\n### Resultado Real de Ejecución en Arch Linux\n```text\n{}\n```",
                        parsed.clean_response,
                        step_observations.trim()
                    );
                    break;
                }

                rolling_user_context.push_str(&format!(
                    "\n\n[Acción ejecutada en Paso {}]:\n{}\n\n[Resultado Real Obtenido en Arch Linux]:\n{}\n\nAnaliza la salida anterior. Si aún faltan archivos por crear, errores de compilación por corregir o falta compilar/instalar en el teléfono (`./gradlew assembleDebug && adb install -r ...`), emite inmediatamente las siguientes herramientas `[TOOL:...]` en este paso. Solo cuando todo el objetivo esté 100% terminado y ejecutándose, presenta el reporte final sin etiquetas `[TOOL:...]`.",
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
                    workspace_path: ws_path_str,
                    workspace_files: ws_files,
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

    // Actualizar lista de archivos del workspace tras el turno
    ws_files = list_workspace_files_recursive(&ws_dir, &ws_dir);

    if !final_clean_answer.trim().is_empty() {
        let final_scope = format!(
            "{}::{}",
            session_id,
            selected_island_slug.clone().unwrap_or_else(|| "global".to_string())
        );
        {
            let mut hist = get_recent_turns().lock().await;
            if hist.len() >= 18 {
                hist.remove(0);
            }
            hist.push((final_scope, msg.to_string(), final_clean_answer.clone()));
        }

        let mem_text = format!(
            "[Workspace: {}] Daniel: {} | Bob: {}",
            ws_dir.display(),
            msg,
            final_clean_answer
        );
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
        workspace_path: ws_path_str,
        workspace_files: ws_files,
        similarity: thal_sim,
        ingest_ms,
        thalamus_us,
        hippocampus_us,
        total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
        causal_edges_total: st.causal_graph.total_edges(),
        tools_used,
    }
}
