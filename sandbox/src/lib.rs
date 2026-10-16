use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct FrictionResult {
    pub compiled_ok: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Lego 4: Cámara de Fricción Bare-Metal (Validación Determinista y Ciclo Hyper-REM)
pub struct FrictionChamber {
    work_dir: PathBuf,
    max_retries: usize,
}

impl FrictionChamber {
    pub fn new(max_retries: usize) -> Self {
        let work_dir = std::env::temp_dir().join("bob_friction_chamber");
        let _ = fs::create_dir_all(&work_dir);
        Self {
            work_dir,
            max_retries,
        }
    }

    pub fn max_retries(&self) -> usize {
        self.max_retries
    }

    /// Oráculo Determinista: Compila y ejecuta código Rust en RAM-disk verificando exit_code == 0
    pub fn verify_rust_code(&self, code: &str, attempt: usize) -> FrictionResult {
        let src_path = self.work_dir.join(format!("friction_attempt_{}.rs", attempt));
        let bin_path = self.work_dir.join(format!("friction_bin_{}", attempt));

        if let Err(e) = fs::write(&src_path, code) {
            return FrictionResult {
                compiled_ok: false,
                exit_code: 1,
                stdout: String::new(),
                stderr: format!("Error escribiendo en tmpfs: {}", e),
            };
        }

        let has_main = code.contains("fn main");
        let compile_res = if has_main {
            Command::new("rustc")
                .arg("-O")
                .arg(&src_path)
                .arg("-o")
                .arg(&bin_path)
                .current_dir(&self.work_dir)
                .output()
        } else {
            Command::new("rustc")
                .arg("--crate-type")
                .arg("lib")
                .arg("--emit=metadata")
                .arg(&src_path)
                .arg("-o")
                .arg(self.work_dir.join(format!("libfriction_{}.rmeta", attempt)))
                .current_dir(&self.work_dir)
                .output()
        };

        match compile_res {
            Ok(out) if out.status.success() => {
                if has_main {
                    match Command::new(&bin_path).current_dir(&self.work_dir).output() {
                        Ok(run_out) => FrictionResult {
                            compiled_ok: run_out.status.success(),
                            exit_code: run_out.status.code().unwrap_or(0),
                            stdout: String::from_utf8_lossy(&run_out.stdout).to_string(),
                            stderr: String::from_utf8_lossy(&run_out.stderr).to_string(),
                        },
                        Err(e) => FrictionResult {
                            compiled_ok: false,
                            exit_code: 1,
                            stdout: String::new(),
                            stderr: format!("Error ejecutando binario validado: {}", e),
                        },
                    }
                } else {
                    FrictionResult {
                        compiled_ok: true,
                        exit_code: 0,
                        stdout: "[Oráculo Determinista: Librería Rust compilada sin errores]".to_string(),
                        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                    }
                }
            }
            Ok(out) => FrictionResult {
                compiled_ok: false,
                exit_code: out.status.code().unwrap_or(1),
                stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            },
            Err(e) => FrictionResult {
                compiled_ok: false,
                exit_code: 1,
                stdout: String::new(),
                stderr: format!("Fallo al invocar el compilador rustc: {}", e),
            },
        }
    }

    /// Construye el prompt de retroalimentación exigiendo código compacto (<35 líneas) con llaves cerradas
    pub fn build_self_correction_prompt(
        original_task: &str,
        failed_code: &str,
        compiler_stderr: &str,
        attempt: usize,
        max_attempts: usize,
    ) -> String {
        format!(
            "System: Eres Bob. Corrige el error de compilación en Rust (Intento {}/{}).\n\
            REGLAS ESTRICTAS: Escribe máximo 35 líneas de código, cero comentarios, cierra todas las llaves `}}` y responde SOLO con el bloque ```rust.\n\n\
            [Tarea]: {}\n\
            [Código Fallido]:\n```rust\n{}\n```\n\
            [Error rustc]:\n{}\n\n\
            Bob:\n<think>\nCorrigiendo código en menos de 35 líneas cerrando fn main.\n</think>\n```rust\n",
            attempt, max_attempts, original_task, failed_code, compiler_stderr
        )
    }

    /// Fase Hyper-REM: Prefill directo a ```rust y límite de 35 líneas para evitar truncamiento
    pub fn build_rem_dream_prompt(concept_a: &str, concept_b: &str) -> String {
        format!(
            "System: [MODO HYPER-REM ACTIVO]\n\
            Eres Bob. Escribe un programa en Rust estándar autocontenido (con `fn main()`, sin crates externos) que combine:\n\
            1) {}\n\
            2) {}\n\
            REGLAS ESTRICTAS: Máximo 35 líneas de código en total, sin comentarios largos, cierra correctamente `fn main()` y usa `println!` para mostrar las métricas.\n\n\
            Bob:\n<think>\nSintetizando código conciso de 30 líneas.\n</think>\n```rust\n",
            concept_a, concept_b
        )
    }
}
