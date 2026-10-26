use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cortex::serde::{Deserialize, Serialize};
use vector_db::causal_plane::{CausalHypergraph, CausalRelation, DynamicIslandManager};
use vector_db::common::types::IslandCategory;
use vector_db::ingestion::embedder::NativeEmbedder;
use vector_db::HippocampusEngine;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "cortex::serde")]
pub struct SkillSummary {
    pub slug: String,
    pub title: String,
    pub path: String,
    pub preview: String,
    pub content: String,
    pub updated_at: u64,
}

pub struct SkillManager {
    skills_dir: PathBuf,
}

impl SkillManager {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/daniel".to_string());
        let skills_dir = PathBuf::from(home).join("proyectos/bob_engine/skills");
        let _ = fs::create_dir_all(&skills_dir);
        let mgr = Self { skills_dir };
        mgr.seed_default_skills();
        mgr
    }

    pub fn normalize_slug(name: &str) -> String {
        name.trim()
            .to_lowercase()
            .trim_end_matches(".md")
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .trim_matches('_')
            .to_string()
    }

    /// Semilla inicial de Skills procedimentales en .md (BOB puede editarlas y crear nuevas)
    fn seed_default_skills(&self) {
        let defaults: &[(&str, &str)] = &[
            (
                "android_studio_pixel10",
                "# Skill: Compilacion y Despliegue Android Studio en Pixel 10 Pro (Arch Linux)\n\
                **Keywords:** android, kotlin, gradle, gradlew, apk, adb, pixel, noise-cat, noise cat, studio, 25.0.2\n\n\
                ## 1. Diagnostico de Entorno en Arch Linux\n\
                - En esta estacion Arch Linux existen OpenJDK 25 y OpenJDK 21 (`/usr/lib/jvm/java-21-openjdk` y `/opt/android-studio/jbr`).\n\
                - **REGLA CRITICA CONTRA ERROR `25.0.2`:** Gradle 8.x falla con `* What went wrong: 25.0.2` si corre con Java 25. SIEMPRE debes fijar en `gradle.properties` la ruta exacta del JDK 21 o JBR:\n\
                  `org.gradle.java.home=/opt/android-studio/jbr` (o `/usr/lib/jvm/java-21-openjdk` si jbr no existe).\n\
                - **REGLA CRITICA DE WRAPPER:** Nunca escribas `https\\\\://` con doble barra en `gradle-wrapper.properties`. Copia directamente `gradlew`, `gradle/wrapper/gradle-wrapper.jar` y `gradle/wrapper/gradle-wrapper.properties` desde `/home/daniel/AndroidStudioProjects/molex_v1`.\n\
                - **REGLA DE SDK:** Escribe `sdk.dir=/home/daniel/Android/Sdk` en `local.properties` y verifica con `ls /home/daniel/Android/Sdk/platforms` que `compileSdk` coincida con una plataforma instalada (ej. `34` o `35`).\n\n\
                ## 2. Pasos Deterministas de Ejecucion\n\
                1. Copiar wrapper funcional: `cp /home/daniel/AndroidStudioProjects/molex_v1/gradlew <proyecto>/ && cp -r /home/daniel/AndroidStudioProjects/molex_v1/gradle <proyecto>/`\n\
                2. Configurar `local.properties` (`sdk.dir=/home/daniel/Android/Sdk`) y `gradle.properties` (`org.gradle.java.home=/opt/android-studio/jbr` o `/usr/lib/jvm/java-21-openjdk`).\n\
                3. Escribir el codigo completo en `MainActivity.kt` y verificar `AndroidManifest.xml`.\n\
                4. Detener demonios viejos y compilar: `./gradlew --stop && ./gradlew assembleDebug`.\n\
                5. Instalar y abrir en el Pixel 10 Pro: `adb install -r app/build/outputs/apk/debug/app-debug.apk && adb shell am start -n <package>/.MainActivity`.\n",
            ),
            (
                "project_planner_abe",
                "# Skill: Planeacion de Proyectos Grandes y Memoria Externa en Workspace\n\
                **Keywords:** planear, arquitectura, proyecto, crear, desarrollar, workspace, aurora, leika, axis\n\n\
                ## 1. Protocolo de Inicio de Proyecto\n\
                1. Inspeccionar la carpeta dedicada de la conversacion (`WORKSPACE_MANIFEST.md` y `ERROR_LOG.md`).\n\
                2. Dividir la arquitectura en modulos pequenos y verificables antes de escribir codigo.\n\
                3. Actualizar `WORKSPACE_MANIFEST.md` con el arbol de archivos y estado de cada modulo.\n\
                4. Tras cada compilacion fallida, registrar la causa raiz en `ERROR_LOG.md` y actualizar la Skill correspondiente con `[TOOL:SAVE_SKILL]`.\n",
            ),
            (
                "gemini_quiron_bridge",
                "# Skill: Sinapsis Externa con Gemini via CDP Nativo en Rust\n\
                **Keywords:** gemini, preguntar, consultar, quiron, navegador, duda, quiz, quizes\n\n\
                ## 1. Protocolo de Consulta Segura a Gemini\n\
                1. Verificar que el Navegador Puente este abierto en el puerto `9222` (`--remote-debugging-port=9222 --remote-allow-origins=*`).\n\
                2. Invocar `[TOOL:ASK_GEMINI query=\"...\"]` enviando exclusivamente la formulacion tecnica sin exponer claves ni datos sensibles.\n\
                3. Para resolver quizes en el navegador, ejecutar `[TOOL:BROWSER_INSPECT]` para leer el DOM de la pestana activa y `[TOOL:BROWSER_CLICK text=\"...\"]` para seleccionar la opcion correcta.\n",
            ),
            (
                "rust_tauri_arch",
                "# Skill: Ingenieria Bare-Metal en Rust y Tauri v2 (Arch Linux + Hyprland)\n\
                **Keywords:** rust, cargo, tauri, hyprland, wayland, simd, vector_db, bob_engine\n\n\
                ## 1. Reglas Inquebrantables\n\
                1. Cero scripts en Python para logica del sistema: todo modulo de BOB debe escribirse en Rust nativo.\n\
                2. Respetar la paleta 100% monocromatica en AXIS UI (`zinc-950`, `zinc-900`, `zinc-800`, `white`) y evitar etiquetas `<select>` nativas en WebKit2GTK.\n\
                3. Validar toda compilacion con `cargo check` o `cargo build --release` garantizando 0 errores y 0 warnings.\n",
            ),
        ];

        for (slug, content) in defaults {
            let p = self.skills_dir.join(format!("{}.md", slug));
            if !p.exists() {
                let _ = fs::write(p, content);
            }
        }
    }

    pub fn save_or_update_skill(
        &self,
        name: &str,
        content: &str,
        hippocampus: &HippocampusEngine,
        causal_graph: &CausalHypergraph,
        custom_islands: &DynamicIslandManager,
        embedder: &NativeEmbedder,
    ) -> Result<SkillSummary, String> {
        let slug = Self::normalize_slug(name);
        if slug.is_empty() {
            return Err("Nombre de skill inválido".to_string());
        }
        let path = self.skills_dir.join(format!("{}.md", slug));
        let unescaped = content
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\\"", "\"");

        fs::write(&path, &unescaped).map_err(|e| e.to_string())?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let summary_text = format!(
            "[Skill Procedimental .md: {}]: {}",
            slug,
            unescaped.chars().take(600).collect::<String>()
        );
        if let Ok(vec) = embedder.generate_vector(&summary_text) {
            let (nid, _) = hippocampus.remember(
                summary_text.clone(),
                vec.clone(),
                Some(IslandCategory::CoreRules),
                5,
                now,
            );
            causal_graph.record_validation(nid);
            causal_graph.link_nodes(
                nid,
                0,
                CausalRelation::Optimiza,
                format!("Skill .md actualizada: {}", slug),
                0.98,
                now,
            );
            custom_islands.register_base_node(nid, "CoreRules", summary_text, &vec, now);
        }

        self.read_skill(&slug)
            .ok_or_else(|| "Error leyendo la skill recién guardada".to_string())
    }

    pub fn read_skill(&self, name_or_slug: &str) -> Option<SkillSummary> {
        let slug = Self::normalize_slug(name_or_slug);
        let path = self.skills_dir.join(format!("{}.md", slug));
        if !path.exists() {
            return None;
        }
        Self::parse_skill_file(&path)
    }

    pub fn list_skills(&self) -> Vec<SkillSummary> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.skills_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Some(sk) = Self::parse_skill_file(&p) {
                        out.push(sk);
                    }
                }
            }
        }
        out.sort_by(|a, b| a.slug.cmp(&b.slug));
        out
    }

    /// Selecciona automáticamente las Skills relevantes para inyectarlas en el prompt del Córtex
    pub fn match_relevant_skills(&self, user_query: &str) -> String {
        let lower = user_query.to_lowercase();
        let all = self.list_skills();
        let mut matched = Vec::new();

        for sk in &all {
            let content_lower = sk.content.to_lowercase();
            let slug_words: Vec<&str> = sk.slug.split('_').filter(|w| w.len() > 2).collect();
            let mut score = 0;

            for sw in slug_words {
                if lower.contains(sw) {
                    score += 3;
                }
            }

            if let Some(kw_line) = content_lower.lines().find(|l| l.contains("keywords:")) {
                for kw in kw_line.split(|c: char| c == ',' || c == ':').skip(1) {
                    let clean_kw = kw.trim().trim_matches('*');
                    if clean_kw.len() > 2 && lower.contains(clean_kw) {
                        score += 2;
                    }
                }
            }

            if score > 0 {
                matched.push((score, sk));
            }
        }

        matched.sort_by(|a, b| b.0.cmp(&a.0));
        if matched.is_empty() {
            let names: Vec<String> = all.iter().map(|s| format!("`{}.md`", s.slug)).collect();
            return format!(
                "[Catálogo de Skills Procedimentales (.md) en Hipocampo]: {}\n(Usa `[TOOL:READ_SKILL name=\"slug\"]` para leer cualquiera o `[TOOL:SAVE_SKILL name=\"slug\" content=\"...\"]` para crear/mejorar una Skill).",
                names.join(", ")
            );
        }

        let mut out = String::from("### [SKILLS PROCEDIMENTALES (.MD) ACTIVADAS DESDE EL HIPOCAMPO]:\n");
        for (_, sk) in matched.into_iter().take(2) {
            out.push_str(&format!(
                "\n--- SKILL `{}.md` ({}) ---\n{}\n",
                sk.slug, sk.path, sk.content
            ));
        }
        out
    }

    fn parse_skill_file(path: &Path) -> Option<SkillSummary> {
        let content = fs::read_to_string(path).ok()?;
        let slug = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("skill")
            .to_string();
        let title = content
            .lines()
            .find(|l| l.starts_with('#'))
            .map(|l| l.trim_start_matches('#').trim().to_string())
            .unwrap_or_else(|| slug.clone());
        let preview: String = content
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(140)
            .collect();
        let updated_at = fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Some(SkillSummary {
            slug,
            title,
            path: path.to_string_lossy().to_string(),
            preview,
            content,
            updated_at,
        })
    }
}
