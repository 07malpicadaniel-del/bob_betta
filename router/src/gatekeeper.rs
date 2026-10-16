use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeLang {
    Rust,
    Bash,
    Python,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskLevel {
    Bajo,
    Medio,
    Critico(String),
}

#[derive(Debug, Clone)]
pub struct ExtractedBlock {
    pub lang: CodeLang,
    pub code: String,
    pub risk: RiskLevel,
}

pub struct ExecutionOutcome {
    pub approved: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub struct Gatekeeper {
    sandbox_dir: PathBuf,
    dpo_log_path: PathBuf,
}

impl Gatekeeper {
    pub fn new(dpo_log_path: &str) -> Self {
        let sandbox_dir = std::env::temp_dir().join("bob_sandbox_exec");
        let _ = fs::create_dir_all(&sandbox_dir);

        Self {
            sandbox_dir,
            dpo_log_path: PathBuf::from(dpo_log_path),
        }
    }

    /// Analiza la respuesta de Bob buscando bloques de código ejecutables (Rust, Bash, Python)
    pub fn scan_executable_blocks(&self, response: &str) -> Vec<ExtractedBlock> {
        let mut blocks = Vec::new();
        let mut lines = response.lines();

        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            let lang_opt = if trimmed.starts_with("```rust") {
                Some(CodeLang::Rust)
            } else if trimmed.starts_with("```bash") || trimmed.starts_with("```sh") {
                Some(CodeLang::Bash)
            } else if trimmed.starts_with("```python") || trimmed.starts_with("```py") {
                Some(CodeLang::Python)
            } else {
                None
            };

            if let Some(lang) = lang_opt {
                let mut code_lines = Vec::new();
                for inner_line in lines.by_ref() {
                    if inner_line.trim() == "```" {
                        break;
                    }
                    code_lines.push(inner_line);
                }
                let code = code_lines.join("\n");
                if !code.trim().is_empty() {
                    let risk = Self::audit_ast_risk(&lang, &code);
                    blocks.push(ExtractedBlock { lang, code, risk });
                }
            }
        }

        blocks
    }

    /// Auditoría estática de firmas peligrosas antes de presentar el switch [Y/N]
    fn audit_ast_risk(lang: &CodeLang, code: &str) -> RiskLevel {
        let critical_patterns = [
            ("rm -rf /", "Borrado recursivo de raíz"),
            ("mkfs", "Formateo de sistema de archivos"),
            ("dd if=", "Escritura directa en bloques de disco"),
            (":(){ :|:& };:", "Fork bomb detectada"),
            ("chmod -R 777 /", "Alteración masiva de permisos"),
        ];

        for (pat, reason) in critical_patterns {
            if code.contains(pat) {
                return RiskLevel::Critico(reason.to_string());
            }
        }

        match lang {
            CodeLang::Bash => {
                if code.contains("sudo ") || code.contains("rm ") || code.contains("pacman ") {
                    RiskLevel::Medio
                } else {
                    RiskLevel::Bajo
                }
            }
            CodeLang::Rust => {
                if code.contains("unsafe ") || code.contains("std::fs::remove") || code.contains("Command::new") {
                    RiskLevel::Medio
                } else {
                    RiskLevel::Bajo
                }
            }
            CodeLang::Python => {
                if code.contains("os.system") || code.contains("subprocess") || code.contains("shutil.rmtree") {
                    RiskLevel::Medio
                } else {
                    RiskLevel::Bajo
                }
            }
        }
    }

    /// Solicita autorización booleana [Y/N] al usuario y ejecuta en el sandbox efímero si se aprueba
    pub fn authorize_and_execute(
        &self,
        prompt: &str,
        block_index: usize,
        block: &ExtractedBlock,
        timestamp: u64,
    ) -> Option<ExecutionOutcome> {
        println!("\n┌──────────────────────────────────────────────────────────────┐");
        println!(
            "│ [GATEKEEPER AST] Bloque #{} detectado ({:?}) — Riesgo: {:?}",
            block_index + 1,
            block.lang,
            block.risk
        );
        println!("└──────────────────────────────────────────────────────────────┘");

        // En Rust solo ofrecemos validar/ejecutar si contiene fn main(), de lo contrario es un snippet declarativo
        if block.lang == CodeLang::Rust && !block.code.contains("fn main") {
            println!("[*] Snippet Rust declarativo (sin `fn main`). ¿Validar sintaxis con `rustc --crate-type lib`? [Y/N/S(kip)] > ");
        } else {
            print!("[?] ¿Autorizar ejecución en Sandbox aislado (/tmp/bob_sandbox_exec)? [Y/N/S(kip)] > ");
        }
        io::stdout().flush().unwrap();

        let mut choice = String::new();
        if io::stdin().read_line(&mut choice).is_err() {
            return None;
        }

        match choice.trim().to_uppercase().as_str() {
            "Y" | "YES" | "SÍ" | "SI" => {
                let outcome = self.execute_in_sandbox(block);
                self.record_dpo_event(prompt, &block.code, true, outcome.exit_code == 0, timestamp);
                Some(outcome)
            }
            "N" | "NO" => {
                println!("[✗] Ejecución rechazada por el usuario. Registrando penalización DPO...");
                self.record_dpo_event(prompt, &block.code, false, false, timestamp);
                Some(ExecutionOutcome {
                    approved: false,
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: "Rechazado por el usuario en el filtro Human-in-the-Loop [N].".to_string(),
                })
            }
            _ => {
                println!("[~] Bloque omitido (sin impacto en DPO).");
                None
            }
        }
    }

    fn execute_in_sandbox(&self, block: &ExtractedBlock) -> ExecutionOutcome {
        match block.lang {
            CodeLang::Bash => {
                let script_path = self.sandbox_dir.join("task.sh");
                let _ = fs::write(&script_path, &block.code);

                let output = Command::new("bash")
                    .arg(&script_path)
                    .current_dir(&self.sandbox_dir)
                    .output();

                Self::format_process_output(output)
            }
            CodeLang::Python => {
                let script_path = self.sandbox_dir.join("task.py");
                let _ = fs::write(&script_path, &block.code);

                let output = Command::new("python3")
                    .arg(&script_path)
                    .current_dir(&self.sandbox_dir)
                    .output();

                Self::format_process_output(output)
            }
            CodeLang::Rust => {
                let src_path = self.sandbox_dir.join("snippet.rs");
                let bin_path = self.sandbox_dir.join("snippet_bin");
                let _ = fs::write(&src_path, &block.code);

                if block.code.contains("fn main") {
                    let compile = Command::new("rustc")
                        .arg(&src_path)
                        .arg("-o")
                        .arg(&bin_path)
                        .current_dir(&self.sandbox_dir)
                        .output();

                    match compile {
                        Ok(c_out) if c_out.status.success() => {
                            let run_out = Command::new(&bin_path)
                                .current_dir(&self.sandbox_dir)
                                .output();
                            Self::format_process_output(run_out)
                        }
                        Ok(c_out) => ExecutionOutcome {
                            approved: true,
                            exit_code: c_out.status.code().unwrap_or(1),
                            stdout: String::from_utf8_lossy(&c_out.stdout).to_string(),
                            stderr: String::from_utf8_lossy(&c_out.stderr).to_string(),
                        },
                        Err(e) => ExecutionOutcome {
                            approved: true,
                            exit_code: 1,
                            stdout: String::new(),
                            stderr: format!("Fallo invocando rustc: {}", e),
                        },
                    }
                } else {
                    // Validación determinista de librería (Oráculo rustc --crate-type lib)
                    let check = Command::new("rustc")
                        .arg("--crate-type")
                        .arg("lib")
                        .arg("--emit=metadata")
                        .arg(&src_path)
                        .arg("-o")
                        .arg(self.sandbox_dir.join("libsnippet.rmeta"))
                        .current_dir(&self.sandbox_dir)
                        .output();

                    Self::format_process_output(check)
                }
            }
        }
    }

    fn format_process_output(res: io::Result<std::process::Output>) -> ExecutionOutcome {
        match res {
            Ok(out) => ExecutionOutcome {
                approved: true,
                exit_code: out.status.code().unwrap_or(0),
                stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            },
            Err(e) => ExecutionOutcome {
                approved: true,
                exit_code: 1,
                stdout: String::new(),
                stderr: format!("Error ejecutando subproceso en Sandbox: {}", e),
            },
        }
    }

    /// Guarda la señal de recompensa DPO en formato JSONL para el ciclo nocturno QLoRA
    fn record_dpo_event(
        &self,
        prompt: &str,
        code: &str,
        approved_by_human: bool,
        compiled_ok: bool,
        timestamp: u64,
    ) {
        let escape = |s: &str| -> String {
            s.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "")
        };

        let status = if approved_by_human && compiled_ok {
            "chosen_verified"
        } else if approved_by_human {
            "chosen_failed_compile"
        } else {
            "rejected_by_human"
        };

        let json_line = format!(
            "{{\"timestamp\":{},\"status\":\"{}\",\"prompt\":\"{}\",\"candidate_code\":\"{}\"}}\n",
            timestamp,
            status,
            escape(prompt),
            escape(code)
        );

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.dpo_log_path)
        {
            let _ = file.write_all(json_line.as_bytes());
        }
    }
}
