use crate::common::types::IslandCategory;

pub struct ThalamusRouter;

impl ThalamusRouter {
    /// Clasificación heurística ultrarrápida de intenciones previa a inferencia profunda.
    /// Enruta el vector hacia la isla correcta sin penalizar latencia en RAM.
    pub fn route_intent(prompt: &str) -> IslandCategory {
        let lower = prompt.to_lowercase();

        // 1. Detección de patrones de código y compilación
        if lower.contains("fn ")
            || lower.contains("impl ")
            || lower.contains("struct ")
            || lower.contains("error[e")
            || lower.contains("cargo")
            || lower.contains("bug")
            || lower.contains("syntax")
            || lower.contains("compile")
            || lower.contains("code")
            || lower.contains("rust")
        {
            return IslandCategory::Code;
        }

        // 2. Detección de reglas operativas, arquitectura e identidad
        if lower.contains("regla")
            || lower.contains("sistema")
            || lower.contains("arquitectura")
            || lower.contains("bob")
            || lower.contains("directiva")
            || lower.contains("filtro")
        {
            return IslandCategory::CoreRules;
        }

        // 3. Memoria temporal de trabajo y contexto inmediato
        if lower.contains("ahora")
            || lower.contains("actual")
            || lower.contains("este comando")
            || lower.contains("salida")
            || lower.contains("último")
        {
            return IslandCategory::WorkingMemory;
        }

        // 4. Por defecto: Memoria episódica de largo plazo
        IslandCategory::EpisodicMemory
    }
}
