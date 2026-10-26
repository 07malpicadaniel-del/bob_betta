use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

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
    ReadSkill { name: String },
    SaveSkill { name: String, content: String },
}

pub struct ToolExecutionResult {
    pub tool_name: &'static str,
    pub output: String,
    pub approved: bool,
}

pub struct AgentToolkit;

impl AgentToolkit {
    pub fn system_prompt_instructions() -> &'static str {
        "Herramientas disponibles en Arch Linux (100% Rust Nativo):\n\
        - Para CREAR O SOBRESCRIBIR ARCHIVOS MULTILÍNEA (SIN ESCAPAR COMILLAS NI SALTOS DE LÍNEA), usa SIEMPRE este formato exacto con bloque de código debajo:\n\
          [TOOL:WRITE_FILE path=\"/ruta/exacta/archivo.rs\"]\n\
          ```rust\n\
          // escribe aquí el código completo multilínea tal cual, sin usar base64 ni escapar nada\n\
          ```\n\
        - [TOOL:EXEC_CMD cmd=\"comando\"] -> Ejecuta un comando bash/cargo/gradle/adb en Arch Linux.\n\
          * IMPORTANTE PARA EJECUTAR JUEGOS O APPS EN LA PC DE DANIEL: Si creas un juego interactivo de terminal (que lee `stdin`), NO lo ejecutes en seco porque `stdin` está cerrado en background; lánzalo abriendo una ventana visible en Hyprland con: `[TOOL:EXEC_CMD cmd=\"(kitty -e /ruta/binario || alacritty -e /ruta/binario || foot /ruta/binario || xterm -e /ruta/binario) >/dev/null 2>&1 &\"]`. Si es una app GUI (egui/eframe/gtk), lánzala en background con `/ruta/binario >/dev/null 2>&1 &`.\n\
        - [TOOL:SYS_INFO] -> Estado en vivo de CPU Ryzen 9, RAM y GPU RTX 5060 Ti.\n\
        - [TOOL:LIST_DIR path=\"ruta\"] -> Lista archivos de un directorio local.\n\
        - [TOOL:READ_FILE path=\"ruta\"] -> Lee un archivo local.\n\
        - [TOOL:SEARCH_CODE path=\"ruta\" pattern=\"texto\"] -> Busca código en el proyecto.\n\
        - [TOOL:WEB_SEARCH query=\"consulta\"] -> Busca información actualizada en Internet.\n\
        - [TOOL:CREATE_ISLAND name=\"nombre\" desc=\"descripción\"] -> Crea una Isla de Vectores aislada en el Hipocampo 4D.\n\
        - [TOOL:ASK_GEMINI query=\"pregunta técnica\"] -> Consulta directamente a Gemini en el navegador de Daniel vía CDP WebSocket en Rust.\n\
        - [TOOL:BROWSER_INSPECT] -> Lee el contenido y botones de la pestaña activa del navegador.\n\
        - [TOOL:BROWSER_CLICK text=\"texto\"] -> Hace clic en una opción o botón dentro del navegador.\n\
        - [TOOL:READ_SKILL name=\"slug\"] -> Lee los pasos de una Skill procedimental (.md) de tu Hipocampo.\n\
        - [TOOL:SAVE_SKILL name=\"slug\" content=\"# Skill...\"] -> Crea o actualiza una Skill (.md) en tu Hipocampo."
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

    /// Parser robusto de herramientas que soporta:
    /// 1) Formato de bloque Markdown debajo de `[TOOL:WRITE_FILE path="..."]` (sin escapar nada, nunca se corta en `]`)
    /// 2) Formato con etiqueta de cierre `[/TOOL]`
    /// 3) Formato inline `content="..."` o `cmd="..."` respetando corchetes internos (`#[allow(...)]`, `[0,1,2]`)
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

            if trimmed.starts_with("WRITE_FILE") {
                if let Some(path) = Self::extract_simple_attr(trimmed, "path") {
                    // Caso A: WRITE_FILE sin atributo content=, seguido de un bloque ```...``` o [/TOOL]
                    let first_line_end = trimmed.find('\n').unwrap_or(trimmed.len());
                    let header_line = &trimmed[..first_line_end];

                    if !header_line.contains("content=\"") {
                        let after_header = &trimmed[first_line_end..];
                        if let Some(fenced) = Self::extract_first_fenced_block(after_header) {
                            calls.push(AgentToolCall::WriteFile {
                                path,
                                content: fenced,
                            });
                            continue;
                        } else if let Some(end_close) = after_header.find("[/TOOL]") {
                            let body = after_header[..end_close].trim().to_string();
                            calls.push(AgentToolCall::WriteFile {
                                path,
                                content: body,
                            });
                            continue;
                        }
                    }

                    // Caso B: WRITE_FILE con atributo content="..." (puede ser multilínea con #[allow] o arreglos)
                    if let Some(content_pos) = trimmed.find("content=\"") {
                        let after_content = &trimmed[content_pos + 9..];
                        let content_extracted = Self::extract_until_closing_quote_bracket(after_content);
                        calls.push(AgentToolCall::WriteFile {
                            path,
                            content: content_extracted,
                        });
                        continue;
                    }
                }
            }

            if trimmed.starts_with("EXEC_CMD") {
                let first_line_end = trimmed.find('\n').unwrap_or(trimmed.len());
                let header_line = &trimmed[..first_line_end];
                if !header_line.contains("cmd=\"") {
                    let after_header = &trimmed[first_line_end..];
                    if let Some(fenced) = Self::extract_first_fenced_block(after_header) {
                        calls.push(AgentToolCall::ExecCmd { cmd: fenced });
                        continue;
                    }
                }
                if let Some(cmd_pos) = trimmed.find("cmd=\"") {
                    let after_cmd = &trimmed[cmd_pos + 5..];
                    let clean_cmd = Self::extract_until_closing_quote_bracket(after_cmd);
                    if !clean_cmd.trim().is_empty() {
                        calls.push(AgentToolCall::ExecCmd { cmd: clean_cmd });
                    }
                    continue;
                }
            }

            if trimmed.starts_with("SAVE_SKILL") {
                if let Some(name) = Self::extract_simple_attr(trimmed, "name") {
                    let first_line_end = trimmed.find('\n').unwrap_or(trimmed.len());
                    let header_line = &trimmed[..first_line_end];
                    if !header_line.contains("content=\"") {
                        let after_header = &trimmed[first_line_end..];
                        if let Some(fenced) = Self::extract_first_fenced_block(after_header) {
                            calls.push(AgentToolCall::SaveSkill {
                                name,
                                content: fenced,
                            });
                            continue;
                        }
                    }
                    if let Some(c_pos) = trimmed.find("content=\"") {
                        let after_c = &trimmed[c_pos + 9..];
                        let clean_c = Self::extract_until_closing_quote_bracket(after_c);
                        calls.push(AgentToolCall::SaveSkill {
                            name,
                            content: clean_c,
                        });
                        continue;
                    }
                }
            }

            let Some(end_tag) = trimmed.find(']') else {
                continue;
            };
            let inside = &trimmed[..end_tag];

            if inside.starts_with("CREATE_ISLAND") {
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
            } else if inside.starts_with("READ_SKILL") {
                if let Some(name) = Self::extract_simple_attr(inside, "name") {
                    calls.push(AgentToolCall::ReadSkill { name });
                }
            }
        }

        calls
    }

    /// Extrae el primer bloque ```...``` que aparece inmediatamente después de `[TOOL:WRITE_FILE path="..."]`
    fn extract_first_fenced_block(s: &str) -> Option<String> {
        let start_fence = s.find("```")?;
        let after_fence = &s[start_fence + 3..];
        let first_nl = after_fence.find('\n')?;
        let code_body = &after_fence[first_nl + 1..];
        let end_fence = code_body.find("```")?;
        Some(code_body[..end_fence].to_string())
    }

    /// Extrae el cuerpo de `content="..."` o `cmd="..."` buscando la secuencia real de cierre `"]` no escapada (`\"`)
    /// sin confundirse jamás con corchetes sueltos `]` como `#![allow(unused_variables)]` o `[0, 1, 2]`.
    fn extract_until_closing_quote_bracket(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut last_valid_close = None;

        for i in 0..bytes.len().saturating_sub(1) {
            if bytes[i] == b'"' && bytes[i + 1] == b']' {
                let is_escaped = i > 0 && bytes[i - 1] == b'\\';
                if !is_escaped {
                    let after = &s[i + 2..];
                    if after.is_empty()
                        || after.starts_with('\n')
                        || after.starts_with('\r')
                        || after.trim_start().starts_with("[TOOL:")
                    {
                        return s[..i].to_string();
                    }
                    last_valid_close = Some(i);
                }
            }
        }

        if let Some(idx) = last_valid_close {
            return s[..idx].to_string();
        }
        s.trim_end_matches(']').trim_end_matches('"').to_string()
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
                output: Self::read_file_safe(path, 160),
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
                output: crate::browser_cdp::RustBrowserBridge::ask_gemini_native(query),
                approved: true,
            },
            AgentToolCall::BrowserInspect => ToolExecutionResult {
                tool_name: "BROWSER_INSPECT",
                output: crate::browser_cdp::RustBrowserBridge::inspect_active_tab(),
                approved: true,
            },
            AgentToolCall::BrowserClick { text } => ToolExecutionResult {
                tool_name: "BROWSER_CLICK",
                output: crate::browser_cdp::RustBrowserBridge::click_element_by_text(text),
                approved: true,
            },
            AgentToolCall::ReadSkill { name } => {
                let mgr = crate::skills::SkillManager::new();
                let out = mgr
                    .read_skill(name)
                    .map(|s| format!("Contenido de Skill `{}.md`:\n{}", s.slug, s.content))
                    .unwrap_or_else(|| format!("La Skill `{}` aún no existe.", name));
                ToolExecutionResult {
                    tool_name: "READ_SKILL",
                    output: out,
                    approved: true,
                }
            }
            AgentToolCall::SaveSkill { name, .. } => ToolExecutionResult {
                tool_name: "SAVE_SKILL",
                output: format!("Skill `{}` lista para persistencia en Hipocampo.", name),
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
                    let unescaped = Self::unescape_code_content(content);
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
                    match Command::new("bash")
                        .arg("-c")
                        .arg(cmd)
                        .stdin(Stdio::null())
                        .output()
                    {
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

    fn unescape_code_content(content: &str) -> String {
        // Si ya viene con saltos de línea reales (bloque fenced ```), solo limpiamos escapes básicos si el LLM escapó comillas
        if content.contains('\n') {
            content.replace("\\\\:", "\\:")
        } else {
            content
                .replace("\\n", "\n")
                .replace("\\t", "\t")
                .replace("\\\"", "\"")
                .replace("\\\\:", "\\:")
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
                let unescaped = Self::unescape_code_content(content);
                if let Some(parent) = Path::new(path).parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = fs::create_dir_all(parent);
                    }
                }
                match fs::write(path, &unescaped) {
                    Ok(_) => ToolExecutionResult {
                        tool_name: "WRITE_FILE",
                        output: format!(
                            "Archivo `{}` escrito exitosamente en disco ({} bytes, {} líneas).",
                            path,
                            unescaped.len(),
                            unescaped.lines().count()
                        ),
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
                let forbidden = [
                    "rm -rf /",
                    "mkfs",
                    "dd if=",
                    ":(){",
                    "chmod -R 777 /",
                    "shutdown",
                    "reboot",
                ];
                for pat in forbidden {
                    if cmd.contains(pat) {
                        return ToolExecutionResult {
                            tool_name: "EXEC_CMD",
                            output: format!("Comando bloqueado por seguridad AST: contiene `{}`.", pat),
                            approved: false,
                        };
                    }
                }

                let env_wrapped_cmd = format!(
                    "if [ -d /opt/android-studio/jbr ]; then export JAVA_HOME=/opt/android-studio/jbr; elif [ -d /usr/lib/jvm/java-21-openjdk ]; then export JAVA_HOME=/usr/lib/jvm/java-21-openjdk; fi; export ANDROID_HOME=$HOME/Android/Sdk; export PATH=$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$PATH; {}",
                    cmd
                );

                match Command::new("bash")
                    .args(["-c", &env_wrapped_cmd])
                    .stdin(Stdio::null())
                    .output()
                {
                    Ok(out) => {
                        let code = out.status.code().unwrap_or(0);
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        let combined = format!(
                            "Exit Code: {} (Ejecución de terminal completada.)\nSTDOUT:\n{}\nSTDERR:\n{}",
                            code,
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
