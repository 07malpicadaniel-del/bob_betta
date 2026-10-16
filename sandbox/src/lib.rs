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
        // En Arch Linux, /tmp está montado sobre tmpfs (RAM-disk de ultra baja latencia)
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

    /// Construye el prompt de retroalimentación cuando el código falla en la Cámara de Fricción
    pub fn build_self_correction_prompt(
        original_task: &str,
        failed_code: &str,
        compiler_stderr: &str,
        attempt: usize,
        max_attempts: usize,
    ) -> String {
        format!(
            "System: Eres Bob. Tu código en Rust falló la validación determinista en la Cámara de Fricción (Intento {}/{}).\n\
            Analiza el error exacto del compilador `rustc`, corrige el código y devuelve ÚNICAMENTE el bloque ```rust corregido con `fn main()` que compile sin errores y sin usar crates externos.\n\n\
            [Tarea Original]: {}\n\n\
            [Código Fallido]:\n```rust\n{}\n```\n\n\
            [STDERR del Compilador rustc]:\n{}\n\n\
            Bob:",
            attempt, max_attempts, original_task, failed_code, compiler_stderr
        )
    }

    /// Fase Hyper-REM: Genera una semilla de colisión sintética entre dos conceptos del Hipocampo
    pub fn build_rem_dream_prompt(concept_a: &str, concept_b: &str) -> String {
        format!(
            "System: [MODO HYPER-REM: SUEÑO ALGORÍTMICO ACTIVO]\n\
            Eres Bob. Estás en tu ciclo de simulación nocturna en la Cámara de Fricción.\n\
            Debes sintetizar un programa autocontenido en Rust estándar (con `fn main()` y `assert!`, sin crates externos) \
            que modele y demuestre algorítmicamente la unión eficiente de estos dos axiomas de tu Hipocampo:\n\
            - Axioma A: {}\n\
            - Axioma B: {}\n\
            Escribe un bloque ```rust limpio, rápido y determinista que compile a la primera con `rustc`.\n\
            Bob:",
            concept_a, concept_b
        )
    }
}
