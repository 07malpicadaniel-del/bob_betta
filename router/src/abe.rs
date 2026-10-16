use std::time::Instant;

use cortex::kv_cache::KvCacheManager;
use cortex::CortexEngine;
use sandbox::FrictionChamber;

use crate::gatekeeper::Gatekeeper;
use crate::thought_buffer::ThoughtBuffer;

pub struct ScenarioBranch {
    pub name: &'static str,
    pub strategy: &'static str,
    pub code: String,
    pub compiled_ok: bool,
    pub attempts_used: usize,
    pub stdout: String,
    pub stderr: String,
    pub eval_micros: u128,
    pub score: f64,
}

pub struct AbeWinner {
    pub winning_branch: ScenarioBranch,
    pub pruned_branch_name: &'static str,
    pub kv_shared_bytes: usize,
}

/// Motor de Árbol Binario de Escenarios (ABE) con Prefix Tree Sharing en KV Cache
pub struct BinaryScenarioTree;

impl BinaryScenarioTree {
    pub fn execute_deep_think(
        problem: &str,
        hippocampus_context: &str,
        cortex: &CortexEngine,
        kv_manager: &KvCacheManager,
        friction: &FrictionChamber,
        gatekeeper: &Gatekeeper,
        thought_buffer: &mut ThoughtBuffer,
        extract_fn: fn(&str, &Gatekeeper) -> Option<String>,
    ) -> Result<AbeWinner, String> {
        println!("\n┌────────────────────────────────────────────────────────────────────┐");
        println!("│ [MODO /deep-think — ÁRBOL BINARIO DE ESCENARIOS (ABE)]             │");
        println!("└────────────────────────────────────────────────────────────────────┘");

        // 1. Guardar el estado raíz del KV Cache en RAM-disk (/tmp/bob_kv_slots)
        let snapshot_file = "abe_root_trunk.bin";
        let kv_shared_bytes = match kv_manager.save_slot(0, snapshot_file) {
            Ok(res) => {
                let saved = res.n_saved.unwrap_or(0);
                println!(
                    "[*] Tronco común del KV Cache congelado en RAM-disk (`{}`: {} tokens).",
                    snapshot_file, saved
                );
                saved
            }
            Err(_) => {
                println!("[*] Inicializando tronco común en Slot 0 de la RTX 5060 Ti...");
                0
            }
        };

        // 2. Fase de Expansión: Bifurcación en Escenario A y Escenario B
        let strategy_a = "Enfoque A (Memoria Estática / Zero-Allocation / Arrays Fijos en Stack)";
        let strategy_b = "Enfoque B (Iteradores Funcionales / Procesamiento Vectorizado por Bloques)";

        println!("[*] Expandiendo Rama 1 -> {}...", strategy_a);
        let prompt_a = Self::build_branch_prompt(problem, hippocampus_context, strategy_a);
        let raw_a = cortex.generate_branch(&prompt_a, 0.20)?;
        let parsed_a = thought_buffer.process_output(&raw_a);
        let code_a = extract_fn(&parsed_a.clean_response, gatekeeper)
            .unwrap_or_else(|| parsed_a.clean_response.clone());

        let branch_a = Self::evaluate_and_heal_branch(
            "Escenario A",
            strategy_a,
            problem,
            code_a,
            100,
            cortex,
            friction,
            gatekeeper,
            thought_buffer,
            extract_fn,
        );

        // Restaurar el tronco común del KV Cache antes de evaluar el Escenario B
        if kv_shared_bytes > 0 {
            if let Ok(restored) = kv_manager.restore_slot(0, snapshot_file) {
                println!(
                    "[*] Bifurcación KV Cache: Tronco común restaurado en Slot 0 ({:?} tokens reutilizados).",
                    restored.n_restored
                );
            }
        }

        println!("[*] Expandiendo Rama 2 -> {}...", strategy_b);
        let prompt_b = Self::build_branch_prompt(problem, hippocampus_context, strategy_b);
        let raw_b = cortex.generate_branch(&prompt_b, 0.35)?;
        let parsed_b = thought_buffer.process_output(&raw_b);
        let code_b = extract_fn(&parsed_b.clean_response, gatekeeper)
            .unwrap_or_else(|| parsed_b.clean_response.clone());

        let branch_b = Self::evaluate_and_heal_branch(
            "Escenario B",
            strategy_b,
            problem,
            code_b,
            200,
            cortex,
            friction,
            gatekeeper,
            thought_buffer,
            extract_fn,
        );

        // 3. Reporte Comparativo del Oráculo Determinista
        println!("[*] Resultados del Oráculo Determinista (`rustc -O`):");
        println!(
            "    ├─ {}: Compiló={} (Intentos={}) | Tiempo={} µs | Score={:.2}",
            branch_a.name, branch_a.compiled_ok, branch_a.attempts_used, branch_a.eval_micros, branch_a.score
        );
        println!(
            "    └─ {}: Compiló={} (Intentos={}) | Tiempo={} µs | Score={:.2}",
            branch_b.name, branch_b.compiled_ok, branch_b.attempts_used, branch_b.eval_micros, branch_b.score
        );

        // 4. El Desalojo (Pruning): Podar la rama débil y aislar la trayectoria ideal
        if !branch_a.compiled_ok && !branch_b.compiled_ok {
            return Err(format!(
                "Ambas ramas colapsaron tras auto-corrección.\nError A: {}\nError B: {}",
                branch_a.stderr.lines().next().unwrap_or(""),
                branch_b.stderr.lines().next().unwrap_or("")
            ));
        }

        let (winner, pruned_name) = if branch_a.score >= branch_b.score {
            (branch_a, branch_b.name)
        } else {
            (branch_b, branch_a.name)
        };

        println!(
            "[✂] Poda Sináptica (ABE): `{}` desalojado de la memoria. Ganador: `{}`.",
            pruned_name, winner.name
        );

        Ok(AbeWinner {
            winning_branch: winner,
            pruned_branch_name: pruned_name,
            kv_shared_bytes,
        })
    }

    fn build_branch_prompt(problem: &str, context: &str, strategy: &str) -> String {
        format!(
            "System: [MODO ABE DEEP-THINK]\n\
            Eres Bob. Resuelve el problema aplicando la arquitectura: {}.\n\
            Contexto:\n{}\n\
            REGLAS ESTRICTAS: Programa en Rust estándar autocontenido (con `fn main()`, sin crates externos), \
            máximo 25 líneas, declara `mut` en las variables mutables, cierra todas las llaves `}}` e imprime métricas con `println!`.\n\n\
            Problema: {}\n\n\
            Bob:\n<think>\nImplementación directa de 20 líneas bajo {}.\n</think>\n```rust\n",
            strategy, context, problem, strategy
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_and_heal_branch(
        name: &'static str,
        strategy: &'static str,
        problem: &str,
        initial_code: String,
        base_id: usize,
        cortex: &CortexEngine,
        friction: &FrictionChamber,
        gatekeeper: &Gatekeeper,
        thought_buffer: &mut ThoughtBuffer,
        extract_fn: fn(&str, &Gatekeeper) -> Option<String>,
    ) -> ScenarioBranch {
        let mut current_code = initial_code;
        let max_branch_attempts = 2usize;

        for attempt in 1..=max_branch_attempts {
            let start = Instant::now();
            let res = friction.verify_rust_code(&current_code, base_id + attempt);
            let elapsed_micros = start.elapsed().as_micros();

            if res.compiled_ok {
                let lines = current_code.lines().count().max(1) as f64;
                let retry_penalty = (attempt as f64 - 1.0) * 150.0;
                let score = 1000.0 - retry_penalty - (elapsed_micros as f64 / 1000.0) - (lines * 0.5);

                return ScenarioBranch {
                    name,
                    strategy,
                    code: current_code,
                    compiled_ok: true,
                    attempts_used: attempt,
                    stdout: res.stdout,
                    stderr: res.stderr,
                    eval_micros: elapsed_micros,
                    score,
                };
            }

            if attempt < max_branch_attempts {
                let first_err = res.stderr.lines().next().unwrap_or("Error de compilación");
                println!(
                    "    [!] {} requirió micro-corrección en Cámara de Fricción ({})...",
                    name, first_err
                );
                let fix_prompt = FrictionChamber::build_self_correction_prompt(
                    problem,
                    &current_code,
                    &res.stderr,
                    attempt + 1,
                    max_branch_attempts,
                );
                if let Ok(fix_raw) = cortex.generate_branch(&fix_prompt, 0.15) {
                    let fix_parsed = thought_buffer.process_output(&fix_raw);
                    if let Some(healed_code) = extract_fn(&fix_parsed.clean_response, gatekeeper) {
                        current_code = healed_code;
                    }
                }
            } else {
                return ScenarioBranch {
                    name,
                    strategy,
                    code: current_code,
                    compiled_ok: false,
                    attempts_used: attempt,
                    stdout: res.stdout,
                    stderr: res.stderr,
                    eval_micros: elapsed_micros,
                    score: -1000.0,
                };
            }
        }

        unreachable!()
    }
}
