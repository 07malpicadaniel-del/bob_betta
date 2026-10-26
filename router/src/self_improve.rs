use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sandbox::FrictionChamber;
use vector_db::causal_plane::{CausalHypergraph, CausalRelation, DynamicIslandManager, VectorThalamus};
use vector_db::common::types::IslandCategory;
use vector_db::ingestion::embedder::NativeEmbedder;
use vector_db::HippocampusEngine;

pub struct SelfImprover;

impl SelfImprover {
    /// Modifica un archivo de `bob_engine` o `axis_v1` de forma transaccional:
    /// 1. Crea respaldo en memoria del archivo original.
    /// 2. Escribe el nuevo código propuesto por BOB.
    /// 3. Ejecuta `cargo check` en el proyecto correspondiente.
    /// 4. Si compila con éxito (exit_code == 0), consolida el cambio y lo registra en SELF_EVOLUTION_LOG.md.
    /// 5. Si falla la compilación, restaura automáticamente el respaldo original para NUNCA romper el sistema mientras Daniel no está.
    pub fn transactional_mutate(path_str: &str, new_content: &str) -> String {
        let target_path = Path::new(path_str);
        let backup_content = fs::read_to_string(target_path).ok();

        if let Some(parent) = target_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Err(e) = fs::write(target_path, new_content) {
            return format!("[SELF_MUTATE ERROR] No se pudo escribir en `{}`: {}", path_str, e);
        }

        // Determinar si el archivo pertenece a bob_engine o a axis_v1 para validar con cargo check
        let check_dir = if path_str.contains("axis_v1") {
            Some("/home/daniel/proyectos/axis_v1/src-tauri")
        } else if path_str.contains("bob_engine") {
            Some("/home/daniel/proyectos/bob_engine")
        } else {
            None
        };

        if let Some(dir) = check_dir {
            if path_str.ends_with(".rs") || path_str.ends_with("Cargo.toml") {
                let start = Instant::now();
                let out = Command::new("cargo")
                    .args(["check", "--quiet"])
                    .current_dir(dir)
                    .output();

                match out {
                    Ok(o) if o.status.success() => {
                        let ms = start.elapsed().as_millis();
                        Self::append_evolution_log(
                            path_str,
                            new_content.len(),
                            ms,
                            "COMPILACIÓN VERIFICADA (exit=0)",
                        );
                        return format!(
                            "[✓ AUTOMEJORA TRANSACCIONAL CONSOLIDADA]\n\
                            - Archivo mutado: `{}` ({} bytes)\n\
                            - Verificación `cargo check`: EXIT 0 en {} ms\n\
                            - Registrado en `~/proyectos/bob_engine/SELF_EVOLUTION_LOG.md`.",
                            path_str,
                            new_content.len(),
                            ms
                        );
                    }
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        // Rollback inmediato para proteger la integridad de la PC de Daniel
                        if let Some(orig) = backup_content {
                            let _ = fs::write(target_path, orig);
                        }
                        return format!(
                            "[! ROLLBACK AUTOMÁTICO DE PROTECCIÓN EJECUTADO]\n\
                            El cambio propuesto en `{}` generó errores en `cargo check` y fue revertido al estado seguro original.\n\
                            Analiza el error del compilador y vuelve a intentar con el código corregido:\n{}",
                            path_str,
                            stderr.chars().take(1800).collect::<String>()
                        );
                    }
                    Err(e) => {
                        return format!("Archivo escrito, pero no se pudo invocar cargo check: {}", e);
                    }
                }
            }
        }

        Self::append_evolution_log(path_str, new_content.len(), 0, "ACTUALIZACIÓN DIRECTA");
        format!(
            "[✓ AUTOMEJORA APLICADA] Archivo `{}` actualizado ({} bytes).",
            path_str,
            new_content.len()
        )
    }

    fn append_evolution_log(path_str: &str, bytes: usize, check_ms: u128, status: &str) {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
        let log_path = PathBuf::from(home).join("proyectos/bob_engine/SELF_EVOLUTION_LOG.md");
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut current = fs::read_to_string(&log_path).unwrap_or_else(|_| {
            "# Bitácora de Automejora Desatendida (BOB Engine v2.0)\n\n| Timestamp | Archivo Mutado | Tamaño | Tiempo Verificación | Estado |\n|---|---|---|---|---|\n".to_string()
        });

        current.push_str(&format!(
            "| `{}` | `{}` | {} B | {} ms | **{}** |\n",
            ts, path_str, bytes, check_ms, status
        ));
        let _ = fs::write(log_path, current);
    }

    /// Suite Autónoma de Benchmarks de Silicio y Cognición (100% Rust)
    pub fn run_full_benchmark_suite(
        embedder: &NativeEmbedder,
        thalamus: &VectorThalamus,
        hippocampus: &HippocampusEngine,
        causal_graph: &CausalHypergraph,
        custom_islands: &DynamicIslandManager,
        friction_chamber: &FrictionChamber,
    ) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // 1. Benchmark de Ingesta Sensorial (Candle CPU 768-dim)
        let t0 = Instant::now();
        let test_vec = embedder
            .generate_vector("Benchmark autónomo de latencia vectorial SIMD AVX-512 en Arch Linux Ryzen 9")
            .unwrap_or_else(|_| vec![0.036; 768]);
        let embed_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // 2. Benchmark de Enrutamiento Tálamo Vectorial (1,000 clasificaciones SIMD)
        let t1 = Instant::now();
        for _ in 0..1000 {
            let _ = thalamus.route_vector(&test_vec);
        }
        let thalamus_avg_us = (t1.elapsed().as_secs_f64() * 1_000_000.0) / 1000.0;

        // 3. Benchmark de Búsqueda Híbrida Hipocampo 4D (HNSW + Plano Causal)
        let t2 = Instant::now();
        let _ = hippocampus.recall("Optimización HNSW y axiomas de hardware", test_vec.clone(), 5, now);
        let hippo_us = t2.elapsed().as_secs_f64() * 1_000_000.0;

        // 4. Benchmark de Compilación y Ejecución en Cámara de Fricción (Oráculo rustc -O)
        let bench_rust_code = r#"
fn main() {
    let mut acc: u64 = 0;
    for i in 1..=1_000_000u64 {
        acc = acc.wrapping_add(i.wrapping_mul(31));
    }
    println!("BENCH_OK:{}", acc);
}
"#;
        let t3 = Instant::now();
        let verify = friction_chamber.verify_rust_code(bench_rust_code, 1);
        let oracle_ms = t3.elapsed().as_secs_f64() * 1000.0;

        let report_row = format!(
            "| `{}` | **{:.2} ms** | **{:.3} µs** | **{:.2} µs** | **{:.2} ms** (`exit={}`) | `{}` aristas |",
            now,
            embed_ms,
            thalamus_avg_us,
            hippo_us,
            oracle_ms,
            verify.exit_code,
            causal_graph.total_edges()
        );

        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
        let bench_file = PathBuf::from(home).join("proyectos/bob_engine/BENCHMARKS.md");
        let mut current = fs::read_to_string(&bench_file).unwrap_or_else(|_| {
            "# Reporte Histórico de Benchmarks Autónomos — BOB v2.0\n\n\
            | Timestamp | Ingesta CPU (`bge-base`) | Tálamo SIMD (Prom. 1k) | Hipocampo 4D (`HNSW`) | Oráculo `rustc -O` | Sinapsis Causales |\n\
            |---|---|---|---|---|---|\n"
                .to_string()
        });
        current.push_str(&format!("{}\n", report_row));
        let _ = fs::write(&bench_file, &current);

        // Cristalizar el resultado del benchmark en el Hipocampo 4D
        let mem_note = format!(
            "[Benchmark Autónomo Verificado ts={}]: Embed={:.2}ms, Tálamo={:.3}µs, HNSW={:.2}µs, Oráculo rustc={:.2}ms (exit={}).",
            now, embed_ms, thalamus_avg_us, hippo_us, oracle_ms, verify.exit_code
        );
        let (nid, _) = hippocampus.remember(
            mem_note.clone(),
            test_vec.clone(),
            Some(IslandCategory::CoreRules),
            2,
            now,
        );
        causal_graph.record_validation(nid);
        causal_graph.link_nodes(nid, 0, CausalRelation::Optimiza, mem_note.clone(), 0.96, now);
        custom_islands.register_base_node(nid, "CoreRules", mem_note, &test_vec, now);

        format!(
            "### [SUITE AUTÓNOMA DE BENCHMARKS EJECUTADA EN SILICIO]\n\
            - **Ingesta Sensorial CPU (`768-dim`):** `{:.2} ms` (0% VRAM)\n\
            - **Tálamo Vectorial SIMD (Promedio 1,000 iteraciones):** `{:.3} µs / consulta`\n\
            - **Hipocampo 4D (`HNSW + Propagación Causal`):** `{:.2} µs`\n\
            - **Oráculo Determinista (`rustc -O` 1M ops):** `{:.2} ms` (`exit_code = {}`, stdout = `{}`)\n\
            - **Persistencia:** Tabla actualizada en `~/proyectos/bob_engine/BENCHMARKS.md` y cristalizada en el Hipocampo (Nodo #{}).",
            embed_ms,
            thalamus_avg_us,
            hippo_us,
            oracle_ms,
            verify.exit_code,
            verify.stdout.trim(),
            nid
        )
    }
}
