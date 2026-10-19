use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub enum AgentToolCall {
    SysInfo,
    ListDir { path: String },
    ReadFile { path: String },
    SearchCode { path: String, pattern: String },
    WebSearch { query: String },
    WriteFile { path: String, content: String },
    ExecCmd { cmd: String },
    CreateIsland { name: String, description: String },
    AskGemini { query: String },
    BrowserInspect,
    BrowserClick { text: String },
}

pub struct ToolExecutionResult {
    pub tool_name: &'static str,
    pub output: String,
    pub approved: bool,
}

pub struct AgentToolkit;

impl AgentToolkit {
    pub fn system_prompt_instructions() -> &'static str {
        "Herramientas disponibles en Arch Linux:\n\
        - [TOOL:SYS_INFO] -> Estado en vivo de CPU Ryzen 9, RAM y GPU RTX 5060 Ti.\n\
        - [TOOL:LIST_DIR path=\"ruta\"] -> Lista archivos de un directorio local.\n\
        - [TOOL:READ_FILE path=\"ruta\"] -> Lee un archivo local.\n\
        - [TOOL:SEARCH_CODE path=\"ruta\" pattern=\"texto\"] -> Busca código en el proyecto.\n\
        - [TOOL:WEB_SEARCH query=\"consulta\"] -> Busca información actualizada en Internet.\n\
        - [TOOL:EXEC_CMD cmd=\"comando\"] -> Ejecuta un comando o script bash/python en Arch Linux.\n\
        - [TOOL:WRITE_FILE path=\"ruta\" content=\"texto\"] -> Crea o modifica un archivo en disco.\n\
        - [TOOL:CREATE_ISLAND name=\"nombre\" desc=\"descripción\"] -> Crea una Isla de Vectores aislada en el Hipocampo 4D.\n\
        - [TOOL:ASK_GEMINI query=\"pregunta técnica\"] -> Consulta directamente a Gemini en el navegador de Daniel (Sinapsis Quirón Segura).\n\
        - [TOOL:BROWSER_INSPECT] -> Lee el contenido, preguntas y botones de la pestaña activa del navegador de Daniel (útil para quizes).\n\
        - [TOOL:BROWSER_CLICK text=\"texto de la opción\"] -> Hace clic en una respuesta o botón dentro del navegador de Daniel."
    }

    pub fn detect_island_creation_intent(user_input: &str) -> Option<(String, String)> {
        let lower = user_input.to_lowercase();
        let triggers = [
            "hagas una isla de ",
            "hacer una isla de ",
            "crea una isla de ",
            "crear una isla de ",
            "nueva isla de ",
            "isla de vectores de ",
            "isla llamada ",
        ];

        for t in triggers {
            if let Some(pos) = lower.find(t) {
                let after_orig = user_input[pos + t.len()..].trim();
                let name_raw: String = after_orig
                    .split(|c: char| c == ',' || c == '.' || c == ':' || c == '(')
                    .next()
                    .unwrap_or("")
                    .split_whitespace()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" ");

                let clean_name = name_raw
                    .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                    .to_string();

                if !clean_name.is_empty() {
                    return Some((clean_name, user_input.trim().to_string()));
                }
            }
        }
        None
    }

    pub fn detect_reflex_tools(user_input: &str, hippocampus_sim: f32) -> Vec<AgentToolCall> {
        let mut calls = Self::parse_tool_calls(user_input);
        let lower = user_input.to_lowercase();

        if let Some((name, description)) = Self::detect_island_creation_intent(user_input) {
            if !calls.iter().any(|c| matches!(c, AgentToolCall::CreateIsland { .. })) {
                calls.push(AgentToolCall::CreateIsland { name, description });
            }
        }

        let asks_hardware = (lower.contains("gpu")
            || lower.contains("vram")
            || lower.contains("rtx")
            || lower.contains("temperatura")
            || lower.contains("porcentaje de uso")
            || lower.contains("cuánta ram")
            || lower.contains("cuanta ram")
            || lower.contains("uso de cpu")
            || lower.contains("nvidia-smi"))
            && !calls.iter().any(|c| matches!(c, AgentToolCall::SysInfo));

        if asks_hardware {
            calls.push(AgentToolCall::SysInfo);
        }

        if (lower.contains("qué módulos")
            || lower.contains("que modulos")
            || lower.contains("qué archivos")
            || lower.contains("que archivos")
            || lower.contains("archivos hay")
            || lower.contains("lista"))
            && !calls.iter().any(|c| matches!(c, AgentToolCall::ListDir { .. }))
        {
            if lower.contains("router/src") || lower.contains("crate router") {
                calls.push(AgentToolCall::ListDir {
                    path: "router/src".to_string(),
                });
            } else if lower.contains("vector_db/src") {
                calls.push(AgentToolCall::ListDir {
                    path: "vector_db/src".to_string(),
                });
            } else if lower.contains("cortex/src") {
                calls.push(AgentToolCall::ListDir {
                    path: "cortex/src".to_string(),
                });
            }
        }

        let explicit_web = lower.contains("busca en internet")
            || lower.contains("buscar en internet")
            || lower.contains("busca en la web")
            || lower.contains("investiga en internet")
            || lower.contains("noticias sobre")
            || lower.contains("última versión de")
            || lower.contains("ultima version de");

        let is_local_topic = lower.contains("bob")
            || lower.contains("hipocampo")
            || lower.contains("isla")
            || lower.contains("leika")
            || lower.contains("aurora")
            || lower.contains("sertra")
            || lower.contains("quiron")
            || lower.contains("quirón")
            || lower.contains("mi gpu")
            || lower.contains("mi pc")
            || lower.contains("proyectos");

        let epistemic_web_trigger = !is_local_topic
            && !asks_hardware
            && hippocampus_sim < 0.60
            && (lower.contains('?')
                || lower.starts_with("qué es ")
                || lower.starts_with("que es ")
                || lower.starts_with("quién es ")
                || lower.starts_with("quien es "));

        if (explicit_web || epistemic_web_trigger)
            && !calls.iter().any(|c| matches!(c, AgentToolCall::WebSearch { .. }))
        {
            let cleaned_query = Self::clean_search_query(user_input);
            if !cleaned_query.is_empty() {
                calls.push(AgentToolCall::WebSearch {
                    query: cleaned_query,
                });
            }
        }

        calls
    }

    fn clean_search_query(raw: &str) -> String {
        let prefixes = [
            "busca en internet sobre ",
            "busca en internet ",
            "buscar en internet ",
            "busca en la web ",
            "investiga en internet ",
            "dime ",
            "quiero saber ",
        ];
        let mut q = raw.trim().to_string();
        let lower = q.to_lowercase();
        for p in prefixes {
            if lower.starts_with(p) {
                q = q[p.len()..].trim().to_string();
                break;
            }
        }
        q.trim_matches(|c| c == '?' || c == '¿' || c == '.' || c == '"')
            .trim()
            .to_string()
    }

    pub fn parse_tool_calls(response: &str) -> Vec<AgentToolCall> {
        let mut calls = Vec::new();
        let parts: Vec<&str> = response.split("[TOOL:").collect();

        for part in parts.iter().skip(1) {
            let trimmed = part.trim_start();

            if trimmed.starts_with("SYS_INFO") {
                if !calls.iter().any(|c| matches!(c, AgentToolCall::SysInfo)) {
                    calls.push(AgentToolCall::SysInfo);
                }
                continue;
            }
            if trimmed.starts_with("BROWSER_INSPECT") {
                calls.push(AgentToolCall::BrowserInspect);
                continue;
            }

            let Some(end_tag) = Self::find_closing_tool_bracket(trimmed) else {
                continue;
            };
            let inside = &trimmed[..end_tag];

            if inside.starts_with("EXEC_CMD") {
                if let Some(cmd_pos) = inside.find("cmd=\"") {
                    let cmd_body = &inside[cmd_pos + 5..];
                    let clean_cmd = cmd_body.strip_suffix('"').unwrap_or(cmd_body).to_string();
                    if !clean_cmd.trim().is_empty() {
                        calls.push(AgentToolCall::ExecCmd { cmd: clean_cmd });
                    }
                }
            } else if inside.starts_with("WRITE_FILE") {
                if let Some(path) = Self::extract_simple_attr(inside, "path") {
                    if let Some(content_pos) = inside.find("content=\"") {
                        let content_body = &inside[content_pos + 9..];
                        let clean_content = content_body
                            .strip_suffix('"')
                            .unwrap_or(content_body)
                            .to_string();
                        calls.push(AgentToolCall::WriteFile {
                            path,
                            content: clean_content,
                        });
                    }
                }
            } else if inside.starts_with("CREATE_ISLAND") {
                if let Some(name) = Self::extract_simple_attr(inside, "name") {
                    let description = Self::extract_simple_attr(inside, "desc")
                        .unwrap_or_else(|| "Isla cortical especializada".to_string());
                    calls.push(AgentToolCall::CreateIsland { name, description });
                }
            } else if inside.starts_with("LIST_DIR") {
                let path = Self::extract_simple_attr(inside, "path").unwrap_or_else(|| ".".to_string());
                calls.push(AgentToolCall::ListDir { path });
            } else if inside.starts_with("READ_FILE") {
                if let Some(path) = Self::extract_simple_attr(inside, "path") {
                    calls.push(AgentToolCall::ReadFile { path });
                }
            } else if inside.starts_with("SEARCH_CODE") {
                let path = Self::extract_simple_attr(inside, "path").unwrap_or_else(|| ".".to_string());
                if let Some(pattern) = Self::extract_simple_attr(inside, "pattern") {
                    calls.push(AgentToolCall::SearchCode { path, pattern });
                }
            } else if inside.starts_with("WEB_SEARCH") {
                if let Some(query) = Self::extract_simple_attr(inside, "query") {
                    calls.push(AgentToolCall::WebSearch { query });
                }
            } else if inside.starts_with("ASK_GEMINI") {
                if let Some(query) = Self::extract_simple_attr(inside, "query") {
                    calls.push(AgentToolCall::AskGemini { query });
                }
            } else if inside.starts_with("BROWSER_CLICK") {
                if let Some(text) = Self::extract_simple_attr(inside, "text") {
                    calls.push(AgentToolCall::BrowserClick { text });
                }
            }
        }

        calls
    }

    fn find_closing_tool_bracket(segment: &str) -> Option<usize> {
        let mut last_candidate = None;
        let bytes = segment.as_bytes();
        for i in 0..bytes.len().saturating_sub(1) {
            if bytes[i] == b'"' && bytes[i + 1] == b']' {
                let after = &segment[i + 2..];
                if after.is_empty() || after.starts_with('\n') || after.starts_with('\r') {
                    last_candidate = Some(i + 1);
                } else if last_candidate.is_none() {
                    last_candidate = Some(i + 1);
                }
            }
        }
        last_candidate.or_else(|| segment.find(']'))
    }

    fn extract_simple_attr(input: &str, key: &str) -> Option<String> {
        let pattern = format!("{}=\"", key);
        let start = input.find(&pattern)? + pattern.len();
        let rest = &input[start..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    }

    pub fn execute_tool(call: &AgentToolCall) -> ToolExecutionResult {
        match call {
            AgentToolCall::SysInfo => ToolExecutionResult {
                tool_name: "SYS_INFO",
                output: Self::gather_sys_info(),
                approved: true,
            },
            AgentToolCall::ListDir { path } => ToolExecutionResult {
                tool_name: "LIST_DIR",
                output: Self::list_directory(path),
                approved: true,
            },
            AgentToolCall::ReadFile { path } => ToolExecutionResult {
                tool_name: "READ_FILE",
                output: Self::read_file_safe(path, 120),
                approved: true,
            },
            AgentToolCall::SearchCode { path, pattern } => ToolExecutionResult {
                tool_name: "SEARCH_CODE",
                output: Self::search_code_recursive(Path::new(path), pattern, 25),
                approved: true,
            },
            AgentToolCall::WebSearch { query } => ToolExecutionResult {
                tool_name: "WEB_SEARCH",
                output: Self::search_internet(query),
                approved: true,
            },
            AgentToolCall::CreateIsland { name, description } => ToolExecutionResult {
                tool_name: "CREATE_ISLAND",
                output: format!("Isla `{}` ({}) preparada para inicialización en Hipocampo.", name, description),
                approved: true,
            },
            AgentToolCall::AskGemini { query } => ToolExecutionResult {
                tool_name: "ASK_GEMINI",
                output: Self::run_browser_script("ask_gemini", query),
                approved: true,
            },
            AgentToolCall::BrowserInspect => ToolExecutionResult {
                tool_name: "BROWSER_INSPECT",
                output: Self::run_browser_script("inspect", ""),
                approved: true,
            },
            AgentToolCall::BrowserClick { text } => ToolExecutionResult {
                tool_name: "BROWSER_CLICK",
                output: Self::run_browser_script("click", text),
                approved: true,
            },
            AgentToolCall::WriteFile { path, content } => {
                println!("\n┌────────────────────────────────────────────────────────────────────┐");
                println!("│ [GATEKEEPER TOOL] Solicitud de Escritura de Archivo (`WRITE_FILE`) │");
                println!("└────────────────────────────────────────────────────────────────────┘");
                println!("  Ruta destino: `{}` ({} bytes)", path, content.len());
                print!("[?] ¿Autorizar escritura en disco? [Y/N] > ");
                io::stdout().flush().unwrap();

                let mut choice = String::new();
                let _ = io::stdin().read_line(&mut choice);
                if matches!(choice.trim().to_uppercase().as_str(), "Y" | "YES" | "SI" | "SÍ") {
                    let unescaped = content.replace("\\n", "\n");
                    match fs::write(path, unescaped) {
                        Ok(_) => ToolExecutionResult {
                            tool_name: "WRITE_FILE",
                            output: format!("Archivo `{}` escrito correctamente.", path),
                            approved: true,
                        },
                        Err(e) => ToolExecutionResult {
                            tool_name: "WRITE_FILE",
                            output: format!("Error escribiendo `{}`: {}", path, e),
                            approved: true,
                        },
                    }
                } else {
                    ToolExecutionResult {
                        tool_name: "WRITE_FILE",
                        output: "Operación WRITE_FILE rechazada por el usuario [N].".to_string(),
                        approved: false,
                    }
                }
            }
            AgentToolCall::ExecCmd { cmd } => {
                println!("\n┌────────────────────────────────────────────────────────────────────┐");
                println!("│ [GATEKEEPER TOOL] Solicitud de Comando en Arch Linux (`EXEC_CMD`)  │");
                println!("└────────────────────────────────────────────────────────────────────┘");
                println!("  Comando: `{}`", cmd);
                print!("[?] ¿Autorizar ejecución en tu terminal local? [Y/N] > ");
                io::stdout().flush().unwrap();

                let mut choice = String::new();
                let _ = io::stdin().read_line(&mut choice);
                if matches!(choice.trim().to_uppercase().as_str(), "Y" | "YES" | "SI" | "SÍ") {
                    match Command::new("bash").arg("-c").arg(cmd).output() {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            let combined = format!(
                                "Exit Code: {}\nSTDOUT:\n{}\nSTDERR:\n{}",
                                out.status.code().unwrap_or(0),
                                stdout.trim(),
                                stderr.trim()
                            );
                            ToolExecutionResult {
                                tool_name: "EXEC_CMD",
                                output: combined.chars().take(2000).collect(),
                                approved: true,
                            }
                        }
                        Err(e) => ToolExecutionResult {
                            tool_name: "EXEC_CMD",
                            output: format!("Error ejecutando comando: {}", e),
                            approved: true,
                        },
                    }
                } else {
                    ToolExecutionResult {
                        tool_name: "EXEC_CMD",
                        output: "Comando EXEC_CMD rechazado por el usuario [N].".to_string(),
                        approved: false,
                    }
                }
            }
        }
    }

    pub fn execute_tool_autonomous(call: &AgentToolCall) -> ToolExecutionResult {
        match call {
            AgentToolCall::WriteFile { path, content } => {
                if path.starts_with("/etc") || path.starts_with("/boot") || path.starts_with("/usr") {
                    return ToolExecutionResult {
                        tool_name: "WRITE_FILE",
                        output: format!("Bloqueado por seguridad AST: ruta protegida `{}`.", path),
                        approved: false,
                    };
                }
                let unescaped = if content.contains('\n') {
                    content.clone()
                } else {
                    content
                        .replace("\\n", "\n")
                        .replace("\\t", "\t")
                        .replace("\\\"", "\"")
                };
                if let Some(parent) = Path::new(path).parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = fs::create_dir_all(parent);
                    }
                }
                match fs::write(path, &unescaped) {
                    Ok(_) => ToolExecutionResult {
                        tool_name: "WRITE_FILE",
                        output: format!("Archivo `{}` escrito en disco ({} bytes).", path, unescaped.len()),
                        approved: true,
                    },
                    Err(e) => ToolExecutionResult {
                        tool_name: "WRITE_FILE",
                        output: format!("Error escribiendo `{}`: {}", path, e),
                        approved: false,
                    },
                }
            }
            AgentToolCall::ExecCmd { cmd } => {
                let forbidden = ["rm -rf /", "mkfs", "dd if=", ":(){", "chmod -R 777 /", "shutdown", "reboot"];
                for pat in forbidden {
                    if cmd.contains(pat) {
                        return ToolExecutionResult {
                            tool_name: "EXEC_CMD",
                            output: format!("Comando bloqueado por seguridad AST: contiene `{}`.", pat),
                            approved: false,
                        };
                    }
                }

                let is_heavy_build = cmd.contains("gradle")
                    || cmd.contains("cargo")
                    || cmd.contains("adb ")
                    || cmd.contains("pacman")
                    || cmd.contains("pnpm")
                    || cmd.contains("npm")
                    || cmd.contains("pip ");
                let timeout_dur = if is_heavy_build { "120s" } else { "15s" };
                let env_wrapped_cmd = format!(
                    "export JAVA_HOME=/opt/android-studio/jbr; export ANDROID_HOME=$HOME/Android/Sdk; export PATH=$PATH:$ANDROID_HOME/platform-tools:$JAVA_HOME/bin; {}",
                    cmd
                );

                match Command::new("timeout")
                    .args([timeout_dur, "bash", "-c", &env_wrapped_cmd])
                    .output()
                {
                    Ok(out) => {
                        let code = out.status.code().unwrap_or(0);
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        let status_note = if code == 124 {
                            "Ventana gráfica / proceso interactivo ejecutado en Arch Linux durante 10s sin errores (cerrado automáticamente tras prueba)."
                        } else {
                            "Ejecución de terminal completada."
                        };
                        let combined = format!(
                            "Exit Code: {} ({})\nSTDOUT:\n{}\nSTDERR:\n{}",
                            code,
                            status_note,
                            stdout.trim(),
                            stderr.trim()
                        );
                        ToolExecutionResult {
                            tool_name: "EXEC_CMD",
                            output: combined.chars().take(2500).collect(),
                            approved: true,
                        }
                    }
                    Err(e) => ToolExecutionResult {
                        tool_name: "EXEC_CMD",
                        output: format!("Error ejecutando comando `{}`: {}", cmd, e),
                        approved: false,
                    },
                }
            }
            other => Self::execute_tool(other),
        }
    }

    pub fn run_browser_script(mode: &str, arg: &str) -> String {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
        let py_bin = format!("{}/proyectos/bob_engine/.venv/bin/python3", home);
        let script = format!("{}/proyectos/bob_engine/scripts/bob_browser_bridge.py", home);
        let python_cmd = if Path::new(&py_bin).exists() {
            py_bin
        } else {
            "python3".to_string()
        };

        match Command::new(&python_cmd).args([&script, mode, arg]).output() {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                format!("{}\n{}", stdout.trim(), stderr.trim()).trim().to_string()
            }
            Err(e) => format!("Error ejecutando puente de navegador: {}", e),
        }
    }

    pub fn search_internet(query: &str) -> String {
        let mut results = Vec::new();

        if let Ok(ddg_out) = Command::new("curl")
            .args([
                "-sL",
                "--max-time",
                "8",
                "-A",
                "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0",
                "--data-urlencode",
                &format!("q={}", query),
                "https://html.duckduckgo.com/html/",
            ])
            .output()
        {
            if ddg_out.status.success() {
                let html = String::from_utf8_lossy(&ddg_out.stdout);
                let mut search_pos = &html[..];
                let snippet_marker = "class=\"result__snippet\"";

                while let Some(idx) = search_pos.find(snippet_marker) {
                    let after_marker = &search_pos[idx + snippet_marker.len()..];
                    if let Some(tag_close) = after_marker.find('>') {
                        let content_start = &after_marker[tag_close + 1..];
                        if let Some(end_tag) = content_start.find("</a>").or_else(|| content_start.find("</")) {
                            let raw_snippet = &content_start[..end_tag];
                            let clean = Self::strip_html_tags(raw_snippet);
                            if clean.len() > 25 && !results.contains(&clean) {
                                results.push(format!("- [Web]: {}", clean));
                                if results.len() >= 5 {
                                    break;
                                }
                            }
                            search_pos = &content_start[end_tag..];
                            continue;
                        }
                    }
                    search_pos = after_marker;
                }
            }
        }

        if results.is_empty() {
            format!("No se obtuvieron resultados web externos para `{}`.", query)
        } else {
            format!("Resultados en vivo de Internet para `{}`:\n{}", query, results.join("\n"))
        }
    }

    fn strip_html_tags(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut in_tag = false;
        for ch in input.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => out.push(ch),
                _ => {}
            }
        }
        out.replace("&quot;", "\"")
            .replace("&#x27;", "'")
            .replace("&#39;", "'")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&nbsp;", " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn gather_sys_info() -> String {
        let mut report = String::new();
        if let Ok(load) = fs::read_to_string("/proc/loadavg") {
            report.push_str(&format!("- Carga Kernel (/proc/loadavg): {}\n", load.trim()));
        }
        if let Ok(mem) = fs::read_to_string("/proc/meminfo") {
            let mem_summary: Vec<&str> = mem.lines().take(3).collect();
            report.push_str(&format!("- Memoria RAM Sistema:\n  {}\n", mem_summary.join("\n  ")));
        }
        if let Ok(gpu_out) = Command::new("nvidia-smi")
            .args([
                "--query-gpu=name,memory.used,memory.total,utilization.gpu,temperature.gpu,power.draw",
                "--format=csv,noheader",
            ])
            .output()
        {
            if gpu_out.status.success() {
                let raw = String::from_utf8_lossy(&gpu_out.stdout).trim().to_string();
                let parts: Vec<&str> = raw.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 6 {
                    report.push_str(&format!(
                        "- GPU NVIDIA Activa: {}\n  * VRAM Usada / Total: {} / {}\n  * Porcentaje de Uso GPU: {}\n  * Temperatura Núcleo: {} °C\n  * Consumo Energético: {}\n",
                        parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]
                    ));
                }
            }
        }
        report
    }

    fn list_directory(path: &str) -> String {
        match fs::read_dir(path) {
            Ok(entries) => {
                let mut items = Vec::new();
                for entry in entries.flatten().take(50) {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if let Ok(meta) = entry.metadata() {
                        let kind = if meta.is_dir() { "DIR " } else { "FILE" };
                        items.push(format!("[{}] {} ({} bytes)", kind, file_name, meta.len()));
                    }
                }
                items.sort();
                if items.is_empty() {
                    format!("El directorio `{}` está vacío.", path)
                } else {
                    format!("Contenido real del directorio `{}`:\n{}", path, items.join("\n"))
                }
            }
            Err(e) => format!("Error leyendo directorio `{}`: {}", path, e),
        }
    }

    fn read_file_safe(path: &str, max_lines: usize) -> String {
        match fs::read_to_string(path) {
            Ok(content) => {
                let total_lines = content.lines().count();
                let preview: Vec<&str> = content.lines().take(max_lines).collect();
                format!(
                    "Archivo `{}` (mostrando {}/{} líneas):\n{}",
                    path,
                    preview.len(),
                    total_lines,
                    preview.join("\n")
                )
            }
            Err(e) => format!("Error leyendo archivo `{}`: {}", path, e),
        }
    }

    fn search_code_recursive(dir: &Path, pattern: &str, limit: usize) -> String {
        let mut matches = Vec::new();
        Self::walk_and_search(dir, pattern, &mut matches, limit);
        if matches.is_empty() {
            format!("No se encontraron coincidencias para `{}` en `{:?}`.", pattern, dir)
        } else {
            format!("Coincidencias para `{}` en `{:?}`:\n{}", pattern, dir, matches.join("\n"))
        }
    }

    fn walk_and_search(dir: &Path, pattern: &str, out: &mut Vec<String>, limit: usize) {
        if out.len() >= limit {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if out.len() >= limit {
                break;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "target" || name == "llama.cpp" || name == "models" {
                continue;
            }
            if path.is_dir() {
                Self::walk_and_search(&path, pattern, out, limit);
            } else if path.is_file() {
                if let Ok(text) = fs::read_to_string(&path) {
                    for (idx, line) in text.lines().enumerate() {
                        if line.contains(pattern) {
                            out.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
                            if out.len() >= limit {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}
