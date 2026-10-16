use std::collections::HashMap;
use vector_db::common::types::IslandCategory;

#[derive(Debug, Clone)]
pub enum TriggerReason {
    VacioTopologico(f32),
    FriccionEstancada(usize),
    InvocacionExplicita,
}

impl TriggerReason {
    pub fn description(&self) -> String {
        match self {
            TriggerReason::VacioTopologico(sim) => {
                format!("Vacío Topológico en Hipocampo (similitud máxima = {:.2} < umbral)", sim)
            }
            TriggerReason::FriccionEstancada(intentos) => {
                format!("Fricción Estancada en Sandbox ({} fallos de compilación consecutivos)", intentos)
            }
            TriggerReason::InvocacionExplicita => {
                "Invocación Explícita del Arquitecto (/quiron)".to_string()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ExternalTutor {
    ClaudeOpusSystems,
    GeminiAbstractMath,
}

pub struct SanitizedPacket {
    pub tutor: ExternalTutor,
    pub reason: TriggerReason,
    pub sanitized_code_or_query: String,
    pub socratic_system_prompt: String,
    pub symbol_table_size: usize,
}

/// Lego 5: Protocolo "Tutor Ciego" Quirón (Zero-Knowledge Distillation)
pub struct QuironShield {
    real_to_token: HashMap<String, String>,
    token_to_real: HashMap<String, String>,
    tau_sim_threshold: f32,
}

impl QuironShield {
    pub fn new(tau_sim_threshold: f32) -> Self {
        Self {
            real_to_token: HashMap::new(),
            token_to_real: HashMap::new(),
            tau_sim_threshold,
        }
    }

    /// 1. Disparador de Incertidumbre Epistémica (Fase 6.1)
    pub fn evaluate_trigger(
        &self,
        max_cosine_sim: f32,
        sandbox_failures: usize,
        max_retries: usize,
    ) -> Option<TriggerReason> {
        if sandbox_failures >= max_retries {
            Some(TriggerReason::FriccionEstancada(sandbox_failures))
        } else if max_cosine_sim < self.tau_sim_threshold {
            Some(TriggerReason::VacioTopologico(max_cosine_sim))
        } else {
            None
        }
    }

    /// Selecciona el Tutor Especialista según la Isla Cortical activada en el Tálamo (Fase 6.3)
    pub fn select_tutor(island: IslandCategory) -> ExternalTutor {
        match island {
            IslandCategory::Code | IslandCategory::WorkingMemory => ExternalTutor::ClaudeOpusSystems,
            IslandCategory::CoreRules | IslandCategory::EpisodicMemory => ExternalTutor::GeminiAbstractMath,
        }
    }

    /// 2. Escudo de Sanitización AST y Sustitución Biyectiva en RAM (Fase 6.2)
    pub fn sanitize_context(
        &mut self,
        raw_input: &str,
        island: IslandCategory,
        reason: TriggerReason,
    ) -> SanitizedPacket {
        self.real_to_token.clear();
        self.token_to_real.clear();

        let mut cleaned_lines = Vec::new();
        for line in raw_input.lines() {
            let without_comment = match line.find("//") {
                Some(idx) if !line[..idx].contains("http") => &line[..idx],
                _ => line,
            };
            cleaned_lines.push(without_comment.to_string());
        }
        let mut obfuscated = cleaned_lines.join("\n");

        // Sustitución biyectiva de rutas locales y entidades propietarias
        let sensitive_entities = [
            "/home/daniel/proyectos/bob_engine",
            "/home/daniel",
            "HippocampusEngine",
            "CausalHypergraph",
            "VectorThalamus",
            "NativeEmbedder",
            "CortexEngine",
            "FrictionChamber",
            "AuroraEngine",
            "LeikaHolding",
            "AURORA",
            "aurora",
            "LEIKA",
            "leika",
            "Sertra",
            "sertra",
            "Zuvey",
            "zuvey",
            "Quirón",
            "Quiron",
            "Hipocampo",
            "hipocampo",
            "Bob",
            "BOB",
            "Daniel",
            "daniel",
        ];

        let mut counter = 1usize;
        for entity in sensitive_entities {
            if obfuscated.contains(entity) {
                let token = if entity.starts_with('/') {
                    format!("/workspace/path_{:02}", counter)
                } else {
                    format!("Symbol_Node_{:02}", counter)
                };
                counter += 1;
                obfuscated = obfuscated.replace(entity, &token);
                self.real_to_token.insert(entity.to_string(), token.clone());
                self.token_to_real.insert(token, entity.to_string());
            }
        }

        let tutor = Self::select_tutor(island);
        let socratic_system_prompt = "Actúa como un Oráculo Ciego de Ciencias de la Computación. \
            Resuelve el problema isomórfico adjunto en 3 partes estrictas: \
            (1) Invariante teórico o patrón algorítmico óptimo, \
            (2) Casos borde (edge cases) de memoria o concurrencia, y \
            (3) Implementación de referencia en Rust estándar conservando exactamente los identificadores `Symbol_Node_XX`."
            .to_string();

        SanitizedPacket {
            tutor,
            reason,
            sanitized_code_or_query: obfuscated,
            socratic_system_prompt,
            symbol_table_size: self.token_to_real.len(),
        }
    }

    /// 3. Re-hidratación en RAM: Invierte el mapeo algebraico restaurando nombres y rutas reales
    pub fn rehydrate(&mut self, abstract_response: &str) -> String {
        let mut restored = abstract_response.to_string();
        for (token, real_name) in &self.token_to_real {
            restored = restored.replace(token, real_name);
        }
        self.real_to_token.clear();
        self.token_to_real.clear();
        restored
    }
}
