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
        - [TOOL:SYS_INFO] -> Estado en vivo de CPU Ryzen 9, RAM y GPU RTX 5060 Ti (VRAM, %, temp, watts).\n\
        - [TOOL:LIST_DIR path=\"ruta\"] -> Lista archivos de un directorio local.\n\
        - [TOOL:READ_FILE path=\"ruta\"] -> Lee un archivo local.\n\
        - [TOOL:SEARCH_CODE path=\"ruta\" pattern=\"texto\"] -> Busca código en el proyecto.\n\
        - [TOOL:WEB_SEARCH query=\"consulta\"] -> Busca información actualizada en Internet.\n\
        - [TOOL:EXEC_CMD cmd=\"comando\"] -> Ejecuta un comando en terminal (requiere confirmación [Y/N]).\n\
        - [TOOL:WRITE_FILE path=\"ruta\" content=\"texto\"] -> Crea/modifica un archivo (requiere [Y/N])."
    }

    /// Reflejo Autónomo del Enrutador: Detecta si el usuario pide telemetría de PC, archivos o búsqueda en Internet
    pub fn detect_reflex_tools(user_input: &str, hippocampus_sim: f32) -> Vec<AgentToolCall> {
        let mut calls = Self::parse_tool_calls(user_input);
        let lower = user_input.to_lowercase();

        // 1. Reflejo de Telemetría de Hardware / GPU / VRAM / Temperatura / RAM
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

        // 2. Reflejo de Listado de Directorios si menciona una ruta del proyecto
        if (lower.contains("qué módulos") || lower.contains("que modulos") || lower.contains("qué archivos") || lower.contains("lista"))
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

        // 3. Reflejo de Búsqueda en Internet (explícito o por baja similitud en Hipocampo ante una duda externa)
        let explicit_web = lower.contains("busca en internet")
            || lower.contains("buscar en internet")
            || lower.contains("busca en la web")
            || lower.contains("investiga en internet")
            || lower.contains("en internet")
            || lower.contains("noticias sobre")
            || lower.contains("última versión de")
            || lower.contains("ultima version de");

        let is_local_topic = lower.contains("bob")
            || lower.contains("hipocampo")
            || lower.contains("leika")
            || lower.contains("aurora")
            || lower.contains("sertra")
            || lower.contains("quiron")
            || lower.contains("quirón")
            || lower.contains("mi gpu")
            || lower.contains("mi pc")
            || lower.contains("tu código")
            || lower.contains("tu codigo");

        let epistemic_web_trigger = !is_local_topic
            && !asks_hardware
            && hippocampus_sim < 0.60
            && (lower.contains('?')
                || lower.starts_with("qué es ")
                || lower.starts_with("que es ")
                || lower.starts_with("quién es ")
                || lower.starts_with("quien es ")
                || lower.starts_with("cuál es ")
                || lower.starts_with("cual es ")
                || lower.starts_with("cuándo ")
                || lower.starts_with("cuando "));

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

    /// Extrae todas las invocaciones [TOOL:...] presentes en un texto
    pub fn parse_tool_calls(response: &str) -> Vec<AgentToolCall> {
        let mut calls = Vec::new();
        let mut search_slice = response;

        while let Some(start) = search_slice.find("[TOOL:") {
            let after_start = &search_slice[start + 6..];
            let Some(end) = after_start.find(']') else {
                break;
            };
            let inside = after_start[..end].trim();

            if inside.starts_with("SYS_INFO") {
                if !calls.iter().any(|c| matches!(c, AgentToolCall::SysInfo)) {
                    calls.push(AgentToolCall::SysInfo);
                }
            } else if inside.starts_with("LIST_DIR") {
                let path = Self::extract_attr(inside, "path").unwrap_or_else(|| ".".to_string());
                calls.push(AgentToolCall::ListDir { path });
            } else if inside.starts_with("READ_FILE") {
                if let Some(path) = Self::extract_attr(inside, "path") {
                    calls.push(AgentToolCall::ReadFile { path });
                }
            } else if inside.starts_with("SEARCH_CODE") {
                let path = Self::extract_attr(inside, "path").unwrap_or_else(|| ".".to_string());
                if let Some(pattern) = Self::extract_attr(inside, "pattern") {
                    calls.push(AgentToolCall::SearchCode { path, pattern });
                }
            } else if inside.starts_with("WEB_SEARCH") {
                if let Some(query) = Self::extract_attr(inside, "query") {
                    calls.push(AgentToolCall::WebSearch { query });
                }
            } else if inside.starts_with("WRITE_FILE") {
                if let (Some(path), Some(content)) = (
                    Self::extract_attr(inside, "path"),
                    Self::extract_attr(inside, "content"),
                ) {
                    calls.push(AgentToolCall::WriteFile { path, content });
                }
            } else if inside.starts_with("EXEC_CMD") {
                if let Some(cmd) = Self::extract_attr(inside, "cmd") {
                    calls.push(AgentToolCall::ExecCmd { cmd });
                }
            }

            search_slice = &after_start[end + 1..];
        }

        calls
    }

    fn extract_attr(input: &str, key: &str) -> Option<String> {
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

    /// Buscador de Internet Nativo (DuckDuckGo HTML + Wikipedia API en Español/Inglés)
    pub fn search_internet(query: &str) -> String {
        let mut results = Vec::new();

        // 1. Consulta en vivo a DuckDuckGo HTML
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

        // 2. Respaldo complementario en Wikipedia API (ES) si se requieren definiciones adicionales
        if results.len() < 3 {
            if let Ok(wiki_out) = Command::new("curl")
                .args([
                    "-sL",
                    "--max-time",
                    "6",
                    "-G",
                    "--data-urlencode",
                    "action=query",
                    "--data-urlencode",
                    "list=search",
                    "--data-urlencode",
                    &format!("srsearch={}", query),
                    "--data-urlencode",
                    "format=json",
                    "https://es.wikipedia.org/w/api.php",
                ])
                .output()
            {
                if wiki_out.status.success() {
                    let json_str = String::from_utf8_lossy(&wiki_out.stdout);
                    let mut slice = &json_str[..];
                    let marker = "\"snippet\":\"";
                    while let Some(pos) = slice.find(marker) {
                        let after = &slice[pos + marker.len()..];
                        if let Some(end_q) = after.find("\",\"") {
                            let raw_snip = &after[..end_q];
                            let clean = Self::strip_html_tags(raw_snip);
                            if clean.len() > 20 {
                                results.push(format!("- [Wikipedia]: {}", clean));
                                if results.len() >= 5 {
                                    break;
                                }
                            }
                            slice = &after[end_q..];
                            continue;
                        }
                        break;
                    }
                }
            }
        }

        if results.is_empty() {
            format!("No se obtuvieron resultados web externos para `{}` (verifica conexión de red).", query)
        } else {
            format!(
                "Resultados en vivo de Internet para `{}`:\n{}",
                query,
                results.join("\n")
            )
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
            .replace("\\\"", "\"")
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
                } else {
                    report.push_str(&format!("- GPU Telemetría (nvidia-smi): {}\n", raw));
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
            format!(
                "Coincidencias para `{}` en `{:?}`:\n{}",
                pattern,
                dir,
                matches.join("\n")
            )
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
