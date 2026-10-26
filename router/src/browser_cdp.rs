use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::thread;
use std::time::Duration;

use cortex::serde_json::{self, Value};

pub struct RustBrowserBridge;

impl RustBrowserBridge {
    /// Lanza el navegador en Arch Linux con `--remote-debugging-port=9222` y `--remote-allow-origins=*`
    pub fn launch_browser_bridge() -> String {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
        let profile_dir = format!("{}/.config/bob_browser_profile", home);
        let flags = format!(
            "--remote-debugging-port=9222 --remote-allow-origins=* --user-data-dir='{}' [https://gemini.google.com/app](https://gemini.google.com/app)",
            profile_dir
        );
        let cmd = format!(
            "(google-chrome-stable {} || chromium {} || brave {} || google-chrome {}) >/dev/null 2>&1 &",
            flags, flags, flags, flags
        );
        let _ = Command::new("bash").arg("-c").arg(&cmd).spawn();
        "Navegador Puente (100% Rust CDP) activo en puerto 9222. Abre este chat de Gemini ahí para que BOB pueda contestar directamente.".to_string()
    }

    fn fetch_cdp_tabs() -> Result<Vec<Value>, String> {
        let addr: SocketAddr = "127.0.0.1:9222".parse().unwrap();
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(1200))
            .map_err(|_| "No se detectó navegador en 127.0.0.1:9222. Presiona 'Navegador Puente' en AXIS UI.".to_string())?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(4)));
        let req = "GET /json HTTP/1.1\r\nHost: 127.0.0.1:9222\r\nConnection: close\r\n\r\n";
        stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;

        let mut resp = Vec::new();
        let _ = stream.read_to_end(&mut resp);
        let raw = String::from_utf8_lossy(&resp);
        let body = raw.find("\r\n\r\n").map(|i| &raw[i + 4..]).unwrap_or(&raw);
        serde_json::from_str::<Vec<Value>>(body).map_err(|e| format!("Error leyendo pestañas CDP: {}", e))
    }

    fn cdp_evaluate_js(ws_url: &str, js_expression: &str) -> Result<String, String> {
        let without_proto = ws_url.trim_start_matches("ws://");
        let slash_pos = without_proto.find('/').ok_or("URL WebSocket inválida")?;
        let host_port = &without_proto[..slash_pos];
        let ws_path = &without_proto[slash_pos..];

        let addr: SocketAddr = host_port.parse().map_err(|_| "Host CDP inválido")?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
            .map_err(|e| format!("Fallo conectando WebSocket CDP: {}", e))?;
        let _ = stream.set_read_timeout(None);
        let _ = stream.set_write_timeout(None);

        let handshake = format!(
            "GET {} HTTP/1.1\r\n\
            Host: {}\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 13\r\n\r\n",
            ws_path, host_port
        );
        stream.write_all(handshake.as_bytes()).map_err(|e| e.to_string())?;

        let mut hdr_buf = [0u8; 2048];
        let n = stream.read(&mut hdr_buf).map_err(|e| e.to_string())?;
        let hdr_str = String::from_utf8_lossy(&hdr_buf[..n]);
        if !hdr_str.contains("101") {
            return Err(format!(
                "Handshake WebSocket rechazado: {}",
                hdr_str.lines().next().unwrap_or("")
            ));
        }

        let msg_json = serde_json::json!({
            "id": 1,
            "method": "Runtime.evaluate",
            "params": {
                "expression": js_expression,
                "returnByValue": true,
                "awaitPromise": true
            }
        })
        .to_string();

        Self::write_ws_masked_frame(&mut stream, msg_json.as_bytes())?;

        loop {
            let reply_txt = Self::read_ws_frame(&mut stream)?;
            if let Ok(val) = serde_json::from_str::<Value>(&reply_txt) {
                if val["id"].as_i64() == Some(1) {
                    if let Some(s) = val["result"]["result"]["value"].as_str() {
                        return Ok(s.to_string());
                    }
                    return Ok(val["result"]["result"].to_string());
                }
            }
        }
    }

    fn write_ws_masked_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<(), String> {
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x81);
        let len = payload.len();
        if len <= 125 {
            frame.push(0x80 | (len as u8));
        } else if len <= 65535 {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(len as u64).to_be_bytes());
        }
        let mask: [u8; 4] = [0x37, 0xfa, 0x21, 0x3d];
        frame.extend_from_slice(&mask);
        for (i, &b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        stream.write_all(&frame).map_err(|e| e.to_string())
    }

    fn read_ws_frame(stream: &mut TcpStream) -> Result<String, String> {
        let mut head = [0u8; 2];
        stream.read_exact(&mut head).map_err(|e| e.to_string())?;
        let masked = (head[1] & 0x80) != 0;
        let mut len = (head[1] & 0x7F) as usize;

        if len == 126 {
            let mut ext = [0u8; 2];
            stream.read_exact(&mut ext).map_err(|e| e.to_string())?;
            len = u16::from_be_bytes(ext) as usize;
        } else if len == 127 {
            let mut ext = [0u8; 8];
            stream.read_exact(&mut ext).map_err(|e| e.to_string())?;
            len = u64::from_be_bytes(ext) as usize;
        }

        let mut mask = [0u8; 4];
        if masked {
            stream.read_exact(&mut mask).map_err(|e| e.to_string())?;
        }

        let mut data = vec![0u8; len];
        stream.read_exact(&mut data).map_err(|e| e.to_string())?;
        if masked {
            for i in 0..len {
                data[i] ^= mask[i % 4];
            }
        }
        Ok(String::from_utf8_lossy(&data).to_string())
    }

    /// Lee el último mensaje que Gemini escribió en el hilo activo de la pestaña de Gemini
    pub fn read_last_gemini_message() -> String {
        let Ok(tabs) = Self::fetch_cdp_tabs() else {
            return "Navegador Puente no detectado en puerto 9222.".to_string();
        };
        let Some(tab) = tabs.iter().find(|t| {
            t["type"].as_str() == Some("page")
                && t["url"].as_str().unwrap_or("").contains("gemini.google.com")
        }) else {
            return "No hay pestaña de Gemini abierta en el Navegador Puente.".to_string();
        };
        let Some(ws_url) = tab["webSocketDebuggerUrl"].as_str() else {
            return "Sin WebSocket en pestaña de Gemini.".to_string();
        };

        let js = "(() => {
            const nodes = Array.from(document.querySelectorAll('message-content, model-response, .model-response-text'));
            if (nodes.length === 0) return '(No hay mensajes previos en este chat de Gemini)';
            const last = (nodes[nodes.length - 1].innerText || '').trim();
            return 'URL_HILO: ' + location.href + '\\nULTIMO_MENSAJE_DE_GEMINI:\\n' + last.slice(0, 3500);
        })()";

        Self::cdp_evaluate_js(ws_url, js).unwrap_or_else(|e| format!("Error leyendo chat de Gemini: {}", e))
    }

    /// Escribe un mensaje directamente en el chat activo de Gemini (el mismo hilo abierto por Daniel) y espera la respuesta
    pub fn ask_gemini_native(prompt_text: &str) -> String {
        let tabs = match Self::fetch_cdp_tabs() {
            Ok(t) => t,
            Err(_) => {
                let _ = Self::launch_browser_bridge();
                thread::sleep(Duration::from_secs(3));
                match Self::fetch_cdp_tabs() {
                    Ok(t2) => t2,
                    Err(e) => return e,
                }
            }
        };

        let gemini_tab = tabs.iter().find(|t| {
            t["type"].as_str() == Some("page")
                && t["url"].as_str().unwrap_or("").contains("gemini.google.com")
        });

        let Some(tab) = gemini_tab else {
            return "No se encontró una pestaña abierta en [https://gemini.google.com/app](https://gemini.google.com/app) dentro del Navegador Puente.".to_string();
        };
        let Some(ws_url) = tab["webSocketDebuggerUrl"].as_str() else {
            return "La pestaña de Gemini no expuso webSocketDebuggerUrl.".to_string();
        };

        // Contar cuántas respuestas había antes de enviar para detectar exactamente la nueva respuesta en un chat existente
        let count_js = "document.querySelectorAll('message-content, model-response, .model-response-text').length.toString()";
        let prev_count: usize = Self::cdp_evaluate_js(ws_url, count_js)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);

        let escaped_prompt = serde_json::to_string(prompt_text).unwrap_or_else(|_| "\"Hola\"".to_string());
        let inject_js = format!(
            "(() => {{
                const box = document.querySelector('div.ql-editor[contenteditable=\"true\"], rich-textarea div[contenteditable=\"true\"], div[role=\"textbox\"], textarea');
                if (!box) return 'ERR_NO_BOX';
                box.focus();
                if (box.tagName === 'TEXTAREA') {{
                    box.value = {};
                    box.dispatchEvent(new Event('input', {{ bubbles: true }}));
                }} else {{
                    box.innerText = {};
                    box.dispatchEvent(new InputEvent('input', {{ bubbles: true, data: {} }}));
                }}
                setTimeout(() => {{
                    const sendBtn = document.querySelector('button.send-button, button[aria-label*=\"Send\"], button[aria-label*=\"Enviar\"]');
                    if (sendBtn) {{
                        sendBtn.click();
                    }} else {{
                        box.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }}));
                    }}
                }}, 400);
                return 'SENT_OK';
            }})()",
            escaped_prompt, escaped_prompt, escaped_prompt
        );

        match Self::cdp_evaluate_js(ws_url, &inject_js) {
            Ok(status) if status.contains("ERR_NO_BOX") => {
                return "No se encontró la caja de chat en Gemini.".to_string();
            }
            Err(e) => return format!("Error inyectando mensaje en el chat de Gemini: {}", e),
            _ => {}
        }

        thread::sleep(Duration::from_secs(4));
        let poll_js = format!(
            "(() => {{
                const nodes = Array.from(document.querySelectorAll('message-content, model-response, .model-response-text'));
                if (nodes.length <= {}) return '';
                return (nodes[nodes.length - 1].innerText || '').trim();
            }})()",
            prev_count
        );

        let mut last_txt = String::new();
        let mut stable = 0;
        for _ in 0..300 {
            thread::sleep(Duration::from_millis(1200));
            if let Ok(curr) = Self::cdp_evaluate_js(ws_url, &poll_js) {
                let trimmed = curr.trim().to_string();
                if !trimmed.is_empty() && trimmed == last_txt {
                    stable += 1;
                    if stable >= 3 {
                        break;
                    }
                } else {
                    stable = 0;
                    last_txt = trimmed;
                }
            }
        }

        if last_txt.is_empty() {
            "Mensaje enviado directamente al chat activo de Gemini.".to_string()
        } else {
            format!("=== RESPUESTA EN VIVO DE GEMINI EN EL CHAT ACTIVO ===\n{}", last_txt)
        }
    }

    pub fn inspect_active_tab() -> String {
        let Ok(tabs) = Self::fetch_cdp_tabs() else {
            return "Navegador Puente en puerto 9222 no disponible.".to_string();
        };
        let Some(tab) = tabs.iter().find(|t| {
            let u = t["url"].as_str().unwrap_or("");
            t["type"].as_str() == Some("page")
                && !u.starts_with("devtools://")
                && !u.contains("myaccount.google.com")
                && !u.contains("accounts.google.com")
        }) else {
            return "No hay pestañas activas inspeccionables.".to_string();
        };
        let Some(ws_url) = tab["webSocketDebuggerUrl"].as_str() else {
            return "Sin WebSocket en la pestaña activa.".to_string();
        };

        let js = "(() => {
            const txt = document.body ? document.body.innerText.slice(0, 3500) : '';
            const btns = Array.from(document.querySelectorAll('button, label, [role=\"radio\"], [role=\"button\"], input[type=\"text\"], textarea, a'))
                .map(el => (el.innerText || el.placeholder || el.value || el.getAttribute('aria-label') || '').trim())
                .filter(s => s.length > 0 && s.length < 100)
                .slice(0, 40);
            return 'URL: ' + location.href + '\\nTEXTO EN PANTALLA:\\n' + txt + '\\nELEMENTOS INTERACTIVOS:\\n' + JSON.stringify(btns);
        })()";

        Self::cdp_evaluate_js(ws_url, js).unwrap_or_else(|e| format!("Error inspeccionando pestaña: {}", e))
    }

    pub fn click_element_by_text(target_text: &str) -> String {
        let Ok(tabs) = Self::fetch_cdp_tabs() else {
            return "Navegador Puente en puerto 9222 no disponible.".to_string();
        };
        let Some(tab) = tabs.iter().find(|t| {
            let u = t["url"].as_str().unwrap_or("");
            t["type"].as_str() == Some("page")
                && !u.starts_with("devtools://")
                && !u.contains("myaccount.google.com")
        }) else {
            return "No hay pestaña activa.".to_string();
        };
        let Some(ws_url) = tab["webSocketDebuggerUrl"].as_str() else {
            return "Sin WebSocket.".to_string();
        };

        let escaped = serde_json::to_string(&target_text.to_lowercase()).unwrap_or_default();
        let js = format!(
            "(() => {{
                const target = {};
                const els = Array.from(document.querySelectorAll('button, label, [role=\"radio\"], [role=\"button\"], input, a, span, div'));
                const found = els.find(el => (el.innerText || el.value || '').trim().toLowerCase().includes(target));
                if (!found) return 'No encontrado: ' + target;
                found.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
                found.click();
                return 'Clic ejecutado en: ' + (found.innerText || found.value || target).slice(0, 60);
            }})()",
            escaped
        );

        Self::cdp_evaluate_js(ws_url, &js).unwrap_or_else(|e| format!("Error en clic CDP: {}", e))
    }

    /// Escribe texto en cualquier campo de entrada de un examen/benchmark web y presiona Enter
    pub fn type_into_active_tab(text_to_type: &str) -> String {
        let Ok(tabs) = Self::fetch_cdp_tabs() else {
            return "Navegador Puente en puerto 9222 no disponible.".to_string();
        };
        let Some(tab) = tabs.iter().find(|t| {
            let u = t["url"].as_str().unwrap_or("");
            t["type"].as_str() == Some("page")
                && !u.starts_with("devtools://")
                && !u.contains("myaccount.google.com")
        }) else {
            return "No hay pestaña activa.".to_string();
        };
        let Some(ws_url) = tab["webSocketDebuggerUrl"].as_str() else {
            return "Sin WebSocket.".to_string();
        };

        let escaped = serde_json::to_string(text_to_type).unwrap_or_default();
        let js = format!(
            "(() => {{
                const box = document.activeElement && (document.activeElement.tagName === 'INPUT' || document.activeElement.tagName === 'TEXTAREA' || document.activeElement.isContentEditable)
                    ? document.activeElement
                    : document.querySelector('input[type=\"text\"], textarea, [contenteditable=\"true\"]');
                if (!box) return 'No se encontró campo de texto activo en la página.';
                box.focus();
                if (box.tagName === 'INPUT' || box.tagName === 'TEXTAREA') {{
                    box.value = {};
                    box.dispatchEvent(new Event('input', {{ bubbles: true }}));
                }} else {{
                    box.innerText = {};
                    box.dispatchEvent(new InputEvent('input', {{ bubbles: true, data: {} }}));
                }}
                box.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }}));
                return 'Texto ingresado con éxito en la pestaña activa.';
            }})()",
            escaped, escaped, escaped
        );

        Self::cdp_evaluate_js(ws_url, &js).unwrap_or_else(|e| format!("Error escribiendo en pestaña: {}", e))
    }
}
