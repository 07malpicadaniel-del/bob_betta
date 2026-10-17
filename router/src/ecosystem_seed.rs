use std::fs;
use std::path::Path;

use vector_db::causal_plane::{CausalHypergraph, CausalRelation};
use vector_db::common::types::{IslandCategory, NodeId};
use vector_db::ingestion::embedder::NativeEmbedder;
use vector_db::HippocampusEngine;

pub struct EcosystemSeeder;

impl EcosystemSeeder {
    /// Siembra el contexto maestro de Daniel, el holding LEIKA y todos sus proyectos en el Hipocampo 4D
    pub fn seed_daniel_ecosystem(
        hippocampus: &HippocampusEngine,
        causal_graph: &CausalHypergraph,
        embedder: &NativeEmbedder,
        now: u64,
    ) -> usize {
        let core_axioms: Vec<(IslandCategory, CausalRelation, &str)> = vec![
            (
                IslandCategory::CoreRules,
                CausalRelation::DependeDe,
                "Perfil del Arquitecto (Daniel Garcia Malpica Benitez): Estudiante universitario de Ingeniería en Desarrollo de Software (Tecmilenio) e Ingeniería en Inteligencia Artificial (Hybridge). Desarrollador de sistemas en Rust, C++, Assembly, Kotlin, Python y TypeScript/Next.js/Tauri. Entrena fuerza 4 días por semana en Anytime Fitness y toca guitarra eléctrica, piano y batería.",
            ),
            (
                IslandCategory::CoreRules,
                CausalRelation::DependeDe,
                "Infraestructura Física de Daniel: Estación principal de escritorio con Arch Linux + Hyprland (Wayland), procesador AMD Ryzen 9 7900 (24 hilos lógicos, AVX-512), 64 GB RAM DDR5, GPU NVIDIA GeForce RTX 5060 Ti de 16 GB VRAM y monitor UltraWide de 34 pulgadas. Laptop secundaria con Gentoo Linux, servidor local en Raspberry Pi 4 con Arch Linux ARM y smartphone Google Pixel 10 Pro (Tensor G5, 16 GB RAM). Editor principal: Neovim.",
            ),
            (
                IslandCategory::CoreRules,
                CausalRelation::Implementa,
                "Holding Maestro LEIKA: Estructura corporativa matriz (con planes de LLC en Delaware) creada por Daniel para consolidar sus plataformas de software B2B, motores de inteligencia artificial, automatización e investigación tecnológica.",
            ),
            (
                IslandCategory::Code,
                CausalRelation::Optimiza,
                "Proyecto AXIS (axis_v1 y axis-mobile): Centro de comando modular de Daniel. Versión desktop construida con Tauri v2, Rust, React 19 y Tailwind CSS v4 (crates: axis-core, axis-module-lifestyle para tareas/hábitos/finanzas/agenda/gym, axis-module-library para gestión de repositorios locales y módulo integrado de BOB). Versión móvil nativa en Android con Jetpack Compose, Material 3 y Room.",
            ),
            (
                IslandCategory::Code,
                CausalRelation::Implementa,
                "Proyecto BOB (bob_engine / bob_betta): Oráculo de Dominio Cerrado y agente autónomo 100% Rust sobre Arch Linux. Compuesto por Lego 1 (Hipocampo 4D con islas HNSW en 768 dims + Hipergrafo Causal O(E) + LTP Decay + Tálamo SIMD <3 µs), Lego 2 (Córtex Qwen 27B IQ4_XS con 99 capas en RTX 5060 Ti y KV Cache q8_0), Lego 3 (Router Tokio + Gatekeeper AST [Y/N] + DPO + ABE /deep-think), Lego 4 (Cámara de Fricción rustc en tmpfs + sueño /rem) y Lego 5 (Escudo Zero-Knowledge Quirón).",
            ),
            (
                IslandCategory::Code,
                CausalRelation::Implementa,
                "Proyectos B2B y Cloud de Daniel: AURORA (motor autónomo de ventas B2B con web scraping e inferencia LLM desplegado en Google Cloud Run y Vercel), Sertra (sistema ligero de inventario con consultas en lenguaje natural en Next.js), ObraVoice (app de gestión de construcción con bitácora de voz offline y planos digitales), Tracto-diesel (plataforma B2B de autopartes para camiones en Veracruz con Next.js y Supabase) y FERXARP (plataforma que conecta ONGs con proveedores corporativos).",
            ),
            (
                IslandCategory::Code,
                CausalRelation::Optimiza,
                "Proyectos de Sistemas, IA, Hardware y Videojuegos de Daniel: Molex / molexbox (control remoto de PC desde Android hacia estaciones Linux vía Rust, Uniffi y SSH), vector_DB_ll (base de datos vectorial bare-metal en Rust con paralelismo Rayon), beta_brain67 (simulación de redes neuronales de pulsos Leaky Integrate-and-Fire en Python), NanoTechLab (simulador cuántico a escala atómica en Tauri + Rust + React) y los videojuegos retro de supervivencia The Outpost (con herramienta de assets 16-bit rembrandt) y The Inkscape.",
            ),
        ];

        let mut prev_id: Option<NodeId> = None;
        let mut inserted = 0usize;

        for (island, rel, text) in core_axioms {
            if let Ok(vec) = embedder.generate_vector(text) {
                let (node_id, _) = hippocampus.remember(
                    text.to_string(),
                    vec,
                    Some(island),
                    2,
                    now,
                );
                causal_graph.record_validation(node_id);
                causal_graph.record_validation(node_id);

                if let Some(parent) = prev_id {
                    let short_summary: String = text.chars().take(110).collect();
                    causal_graph.link_nodes(
                        node_id,
                        parent,
                        rel,
                        format!("{}...", short_summary),
                        0.96,
                        now,
                    );
                }
                prev_id = Some(node_id);
                inserted += 1;
            }
        }

        inserted
    }

    /// Escanea dinámicamente ~/proyectos e inyecta cada repositorio encontrado en la Isla Code del Hipocampo
    pub fn scan_and_ingest_local_projects(
        projects_dir: &Path,
        hippocampus: &HippocampusEngine,
        causal_graph: &CausalHypergraph,
        embedder: &NativeEmbedder,
        now: u64,
    ) -> Vec<String> {
        let mut ingested_names = Vec::new();
        let Ok(entries) = fs::read_dir(projects_dir) else {
            return ingested_names;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }

            let mut stack_tags = Vec::new();
            if path.join("Cargo.toml").exists() || path.join("src-tauri/Cargo.toml").exists() {
                stack_tags.push("Rust");
            }
            if path.join("src-tauri").exists() {
                stack_tags.push("Tauri");
            }
            if path.join("package.json").exists() {
                stack_tags.push("TypeScript/Node");
            }
            if path.join("pyproject.toml").exists() || path.join("requirements.txt").exists() {
                stack_tags.push("Python");
            }

            let stack_str = if stack_tags.is_empty() {
                "Repositorio General".to_string()
            } else {
                stack_tags.join(" + ")
            };

            let readme_excerpt = fs::read_to_string(path.join("README.md"))
                .ok()
                .map(|s| {
                    s.lines()
                        .filter(|l| !l.trim().is_empty())
                        .take(4)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_else(|| "Proyecto local en estación Arch Linux de Daniel.".to_string());

            let memory_doc = format!(
                "Proyecto Local Indexado `{}` (Ruta: `{}`, Stack: [{}]): {}",
                name,
                path.display(),
                stack_str,
                readme_excerpt.chars().take(220).collect::<String>()
            );

            if let Ok(vec) = embedder.generate_vector(&memory_doc) {
                let (nid, _) = hippocampus.remember(
                    memory_doc.clone(),
                    vec,
                    Some(IslandCategory::Code),
                    2,
                    now,
                );
                causal_graph.record_validation(nid);
                causal_graph.link_nodes(
                    nid,
                    0,
                    CausalRelation::DependeDe,
                    format!("Proyecto `{}` alojado en estación Arch Linux de Daniel", name),
                    0.92,
                    now,
                );
                ingested_names.push(format!("{} ({})", name, stack_str));
            }
        }

        ingested_names
    }
}
