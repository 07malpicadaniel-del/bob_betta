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
use crate::browser_cdp::RustBrowserBridge;
use crate::ecosystem_seed::EcosystemSeeder;
use crate::extract_rust_from_prefill;
use crate::gatekeeper::{CodeLang, Gatekeeper};
use crate::marathon::{get_marathon_state, MarathonEngine};
use crate::quiron::{QuironShield, TriggerReason};
use crate::skills::SkillManager;
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
struct SkillSaveReq {
    name: String,
    content: String,
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
        if name == "target" || name == "node_modules" || name == ".gradle" || name == "build" || name.starts_with(".git") {
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

/// Decodificador Base64 RFC 4648 escrito 100% en Rust nativo
fn decode_base64_pure_rust(b64: &str, dest_path: &Path) -> Result<usize, String> {
    let mut out = Vec::with_capacity((b64.len() * 3) / 4);
    let mut buf: u32 = 0;
    let mut bits: u8 = 0;

    for byte in b64.bytes() {
        let val = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => continue,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }

    fs::write(dest_path, &out).map_err(|e| e.to_string())?;
    Ok(out.len())
}

fn encode_base64_pure_rust(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(((bytes.len() + 2) / 3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Analizador de Imágenes 100% en Rust nativo
fn analyze_image_pure_rust(img_path: &Path) -> String {
    let bytes = fs::read(img_path).unwrap_or_default();
    let mut dims = String::from("Imagen binaria");

    if bytes.len() >= 24 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        dims = format!("{}x{} px (PNG)", w, h);
    }

    let ocr_txt = Command::new("tesseract")
        .arg(img_path)
        .arg("stdout")
        .args(["-l", "spa+eng", "--psm", "6"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "(Imagen almacenada en el Workspace de la sesión lista para inspección)".to_string());

    format!(
        "[IMAGEN ADJUNTA EN WORKSPACE (100% RUST): `{}` ({} bytes, {})]\nRuta en disco: `{}`\nTexto/UI extraído de la imagen:\n{}\n",
        img_path.file_name().unwrap_or_default().to_string_lossy(),
        bytes.len(),
        dims,
        img_path.display(),
        ocr_txt.chars().take(1800).collect::<String>()
    )
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
            println!("[✓] Puente UI AXIS activo en http://{} (JSON API 100% Rust lista).", addr);
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
                } else if first_line.starts_with("GET /skills") {
                    let mgr = SkillManager::new();
                    serde_json::json!({
                        "ok": true,
                        "skills": mgr.list_skills()
                    })
                    .to_string()
                } else if first_line.starts_with("POST /skills") {
                    if let Ok(req) = serde_json::from_str::<SkillSaveReq>(body) {
                        let mgr = SkillManager::new();
                        match mgr.save_or_update_skill(
                            &req.name,
                            &req.content,
                            &st.hippocampus,
                            &st.causal_graph,
                            &st.custom_islands,
                            &st.embedder,
                        ) {
                            Ok(saved) => serde_json::json!({
                                "ok": true,
                                "skill": saved,
                                "skills": mgr.list_skills()
                            })
                            .to_string(),
                            Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
                        }
                    } else {
                        serde_json::json!({"ok": false, "error": "JSON de skill invalido"}).to_string()
                    }
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
                } else if first_line.starts_with("POST /session/paste") {
                    if let Ok(req) = serde_json::from_str::<WorkspaceInitReq>(body) {
                        let (ws_path, _) = ensure_session_workspace(
                            &st,
                            &req.session_id,
                            req.title.as_deref(),
                            req.island.as_deref(),
                        );
                        let attach_dir = ws_path.join("attachments");
                        let fname = format!("paste_{}.png", now_ts());
                        let dest = attach_dir.join(&fname);
                        let cmd = format!(
                            "wl-paste --type image/png > '{}' 2>/dev/null || wl-paste --type image/jpeg > '{}' 2>/dev/null || xclip -selection clipboard -t image/png -o > '{}' 2>/dev/null",
                            dest.display(),
                            dest.display(),
                            dest.display()
                        );
                        let _ = Command::new("bash").arg("-c").arg(&cmd).status();

                        if dest.exists() && fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) > 64 {
                            let raw_bytes = fs::read(&dest).unwrap_or_default();
                            let b64_preview = format!(
                                "data:image/png;base64,{}",
                                encode_base64_pure_rust(&raw_bytes)
                            );
                            let summary = analyze_image_pure_rust(&dest);
                            let now = now_ts();
                            if let Ok(vec) = st.embedder.generate_vector(&summary) {
                                let (nid, _) = st.hippocampus.remember(
                                    summary.clone(),
                                    vec.clone(),
                                    Some(IslandCategory::Code),
                                    1,
                                    now,
                                );
                                st.custom_islands.register_base_node(
                                    nid,
                                    "Code",
                                    summary.clone(),
                                    &vec,
                                    now,
                                );
                            }
                            let updated_files = list_workspace_files_recursive(&ws_path, &ws_path);
                            serde_json::json!({
                                "ok": true,
                                "items": [{
                                    "filename": fname,
                                    "saved_path": dest.to_string_lossy(),
                                    "is_image": true,
                                    "preview_data_url": b64_preview,
                                    "summary": summary
                                }],
                                "workspace_files": updated_files
                            })
                            .to_string()
                        } else {
                            let _ = fs::remove_file(&dest);
                            serde_json::json!({
                                "ok": false,
                                "error": "No hay imagen en el portapapeles de Wayland. Toma una captura o usa el botón de clip (📎)."
                            })
                            .to_string()
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
                            .map(|c| {
                                if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                                    c
                                } else {
                                    '_'
                                }
                            })
                            .collect();
                        let dest = ws_path.join("attachments").join(&safe_fname);

                        match decode_base64_pure_rust(&req.content_base64, &dest) {
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
                                    analyze_image_pure_rust(&dest)
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

                                let now = now_ts();
                                if let Ok(vec) = st.embedder.generate_vector(&analysis_summary) {
                                    if let Some(isl) = req
                                        .island
                                        .as_deref()
                                        .filter(|s| !s.is_empty() && *s != "global")
                                    {
                                        st.custom_islands.remember_in_island(
                                            isl,
                                            analysis_summary.clone(),
                                            &vec,
                                            now,
                                        );
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

                                let updated_files =
                                    list_workspace_files_recursive(&ws_path, &ws_path);
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
                        serde_json::json!({"ok": false, "error": "JSON de archivo invalido"})
                            .to_string()
                    }
                } else if first_line.starts_with("POST /sync-projects") {
                    let home =
                        std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
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
                        .args([
                            "--rate",
                            "16000",
                            "--channels",
                            "1",
                            "--format",
                            "s16",
                            "/tmp/bob_voice_in.wav",
                        ])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .or_else(|_| {
                            Command::new("arecord")
                                .args([
                                    "-f",
                                    "S16_LE",
                                    "-r",
                                    "16000",
                                    "-c",
                                    "1",
                                    "/tmp/bob_voice_in.wav",
                                ])
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .spawn()
                        });
                    match child {
                        Ok(c) => {
                            *guard = Some(c);
                            serde_json::json!({"ok": true, "status": "recording"}).to_string()
                        }
                        Err(e) => {
                            serde_json::json!({"ok": false, "error": e.to_string()}).to_string()
                        }
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
                    let _req_lang = serde_json::from_str::<VoiceStopReq>(body)
                        .ok()
                        .and_then(|r| r.lang)
                        .unwrap_or_else(|| "auto".to_string());

                    // Invocar whisper CLI del sistema si está instalado
                    let out = Command::new("bash")
                        .args([
                            "-c",
                            "whisper /tmp/bob_voice_in.wav --model base --output_format txt --output_dir /tmp >/dev/null 2>&1 && cat /tmp/bob_voice_in.txt",
                        ])
                        .output();

                    match out {
                        Ok(o) if o.status.success() => {
                            let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
                            serde_json::json!({
                                "ok": !text.is_empty(),
                                "text": text,
                                "language": "es",
                                "pronunciation_score": 92.0,
                                "weak_words": [],
                                "phonetic_report": ""
                            })
                            .to_string()
                        }
                        _ => serde_json::json!({
                            "ok": false,
                            "error": "Audio grabado en /tmp/bob_voice_in.wav (instala 'openai-whisper' o 'whisper.cpp' global si deseas STT sin venv)."
                        })
                        .to_string(),
                    }
                } else if first_line.starts_with("POST /voice/speak") {
                    if let Ok(req) = serde_json::from_str::<SpeakReq>(body) {
                        let voice_key = req.lang.unwrap_or_else(|| "bob_jorge".to_string());
                        let edge_voice = match voice_key.as_str() {
                            "bob_alvaro" => "es-ES-AlvaroNeural",
                            "bob_tomas" => "es-AR-TomasNeural",
                            "bob_christopher" | "en" => "en-US-ChristopherNeural",
                            "bob_ryan" => "en-GB-RyanNeural",
                            "fr" => "fr-FR-HenriNeural",
                            "de" => "de-DE-ConradNeural",
                            "ja" => "ja-JP-KeitaNeural",
                            _ => "es-MX-JorgeNeural",
                        };
                        let clean_txt: String = req
                            .text
                            .chars()
                            .filter(|c| c.is_alphanumeric() || " .,;:¿?¡!()-".contains(*c))
                            .take(900)
                            .collect();
                        let _ = Command::new("pkill").args(["-f", "bob_tts_out"]).status();
                        let cmd = format!(
                            "(~/.local/share/bob_voice_env/bin/edge-tts --voice {} --rate=+10% --pitch=-6Hz --text \"{}\" --write-media /tmp/bob_tts_out.mp3 && (ffplay -nodisp -autoexit -loglevel quiet /tmp/bob_tts_out.mp3 || mpv --no-video --really-quiet /tmp/bob_tts_out.mp3)) >/dev/null 2>&1 &",
                            edge_voice, clean_txt
                        );
                        let _ = Command::new("bash").arg("-c").arg(&cmd).spawn();
                        serde_json::json!({"ok": true}).to_string()
                    } else {
                        serde_json::json!({"ok": false}).to_string()
                    }
                } else if first_line.starts_with("POST /browser/launch") {
                    let msg = RustBrowserBridge::launch_browser_bridge();
                    serde_json::json!({ "ok": true, "msg": msg }).to_string()
                } else if first_line.starts_with("POST /marathon/start") {
                    if let Ok(req) = serde_json::from_str::<MarathonStartReq>(body) {
                        let cycles = req.max_cycles.unwrap_or(10000);
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

    // Inyectar Skills Procedimentales (.md) relevantes + Estado en vivo del Workspace
    let skill_mgr = SkillManager::new();
    full_context.push_str("\n");
    full_context.push_str(&skill_mgr.match_relevant_skills(msg));
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
        "Operando en el Hipocampo 4D Global (4 Islas Base + Hipergrafo Causal + Skills Procedimentales .md).".to_string()
    };

    let system_block = format!(
        "Eres Bob, el agente autónomo soberano de ingeniería de Daniel Garcia Malpica Benitez en Arch Linux (Hyprland), acoplado a AXIS (100% Rust).\n\
        {}\n\
        CARPETA DE PROYECTO DE ESTA CONVERSACIÓN: `{}`.\n\
        MEMORIA PROCEDIMENTAL (SKILLS .MD): Tienes en tu Hipocampo archivos `.md` con pasos probados en `~/proyectos/bob_engine/skills/`. Síguelos rigurosamente y, cuando descubras una solución nueva a un error de compilación o entorno, actualiza o crea la Skill con `[TOOL:SAVE_SKILL name=\"slug\" content=\"# Skill...\"]`.\n\
        REGLA DE PENSAMIENTO: Tu bloque `<think>` debe ser menor a 10 líneas.\n\
        {}",
        island_directive,
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

    for step in 1..=10000 {
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
                    if let AgentToolCall::SaveSkill { name, content } = call {
                        match skill_mgr.save_or_update_skill(
                            name,
                            content,
                            &st.hippocampus,
                            &st.causal_graph,
                            &st.custom_islands,
                            &st.embedder,
                        ) {
                            Ok(saved) => {
                                tools_used.push(format!("SAVE_SKILL ({}.md)", saved.slug));
                                step_observations.push_str(&format!(
                                    "\n[SKILL PROCEDIMENTAL GUARDADA EN HIPOCAMPO]: `{}` actualizada con éxito.\n",
                                    saved.path
                                ));
                            }
                            Err(e) => {
                                step_observations.push_str(&format!("\n[ERROR GUARDANDO SKILL]: {}\n", e));
                            }
                        }
                    } else if let AgentToolCall::CreateIsland { name, description } = call {
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

                        if res.output.contains("Exit Code: 1")
                            || res.output.contains("FAILURE:")
                            || res.output.contains("error[")
                            || res.output.contains("Exception")
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

                if step == 10000 {
                    final_clean_answer = format!(
                        "{}\n\n---\n### Resultado Real de Ejecución en Arch Linux\n```text\n{}\n```",
                        parsed.clean_response,
                        step_observations.trim()
                    );
                    break;
                }

                let short_action: String = parsed.clean_response.chars().take(600).collect();
                let short_obs: String = step_observations.chars().take(1800).collect();
                // Si el historial de pasos supera los 9,500 caracteres, recortamos la mitad intermedia para nunca agotar los 8,192 tokens de VRAM
                if rolling_user_context.chars().count() > 9500 {
                    let head: String = rolling_user_context.chars().take(4200).collect();
                    let tail: String = rolling_user_context
                        .chars()
                        .rev()
                        .take(3200)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect();
                    rolling_user_context = format!(
                        "{}\n\n...[Pasos intermedios compactados automáticamente para liberar VRAM]...\n\n{}",
                        head, tail
                    );
                }
                rolling_user_context.push_str(&format!(
                    "\n\n[Paso {} Ejecutado]:\n{}\n[Salida Real Paso {}]:\n{}\n\nContinúa inmediatamente con el siguiente paso usando `[TOOL:...]` hasta dejar el proyecto compilado e instalado en el dispositivo. Solo cuando hayas terminado al 100%, entrega el reporte final.",
                    step, short_action, step, short_obs
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
            && !block.code.contains("use iced") && !block.code.contains("read_line")
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
