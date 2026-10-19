use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use cortex::serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use vector_db::causal_plane::CausalRelation;
use vector_db::common::types::IslandCategory;

use crate::server::BobSharedState;
use crate::tools::{AgentToolCall, AgentToolkit};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "cortex::serde")]
pub struct MarathonLogEntry {
    pub cycle: usize,
    pub timestamp: u64,
    pub action_summary: String,
    pub tools_executed: Vec<String>,
    pub kv_purged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "cortex::serde")]
pub struct MarathonStatus {
    pub running: bool,
    pub goal: String,
    pub current_cycle: usize,
    pub max_cycles: usize,
    pub axioms_crystallized: usize,
    pub logs: Vec<MarathonLogEntry>,
}

static MARATHON_STATE: std::sync::OnceLock<Mutex<MarathonStatus>> = std::sync::OnceLock::new();

pub fn get_marathon_state() -> &'static Mutex<MarathonStatus> {
    MARATHON_STATE.get_or_init(|| {
        Mutex::new(MarathonStatus {
            running: false,
            goal: String::new(),
            current_cycle: 0,
            max_cycles: 0,
            axioms_crystallized: 0,
            logs: Vec::new(),
        })
    })
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub struct MarathonEngine;

impl MarathonEngine {
    pub async fn start_marathon(st: Arc<BobSharedState>, goal: String, max_cycles: usize) {
        {
            let mut m = get_marathon_state().lock().await;
            if m.running {
                return;
            }
            m.running = true;
            m.goal = goal.clone();
            m.current_cycle = 0;
            m.max_cycles = max_cycles.max(1);
            m.axioms_crystallized = 0;
            m.logs.clear();
        }

        tokio::spawn(async move {
            let mut rolling_progress = format!(
                "OBJETIVO MAESTRO DEL MODO MARATÓN: {}\nIniciando ciclo autónomo.",
                goal
            );

            for cycle in 1..=max_cycles {
                {
                    let mut m = get_marathon_state().lock().await;
                    if !m.running {
                        break;
                    }
                    m.current_cycle = cycle;
                }

                let now = now_sec();
                let q_vec = st
                    .embedder
                    .generate_vector(&rolling_progress)
                    .unwrap_or_default();
                let hippo_ctx = st.hippocampus.recall(&goal, q_vec, 4, now);

                let system_prompt = format!(
                    "Eres Bob operando en MODO MARATÓN AUTÓNOMO (Ciclo {}/{}).\n\
                    Tu objetivo de largo alcance es: \"{}\".\n\
                    En cada ciclo debes avanzar un paso concreto verificable:\n\
                    - Si estás mejorando un proyecto de código (ej. AURORA, Axis, etc.), inspecciona archivos con `[TOOL:LIST_DIR]` / `[TOOL:READ_FILE]`, aplica mejoras reales con `[TOOL:WRITE_FILE]` y compila/verifica con `[TOOL:EXEC_CMD cmd=\"...\"]` autocorrigiendo cualquier error.\n\
                    - Si estás resolviendo quizes o tareas en el navegador de Daniel, usa `[TOOL:BROWSER_INSPECT]` para leer la pregunta actual, razona la respuesta correcta y selecciónala con `[TOOL:BROWSER_CLICK text=\"...\"]`.\n\
                    - Si necesitas apoyo teórico de un modelo masivo externo sin exponer datos privados, usa `[TOOL:ASK_GEMINI query=\"...\"]`.\n\
                    - ÚNICAMENTE cuando hayas completado al 100% todo el objetivo maestro, escribe la etiqueta `[MARATHON_COMPLETE]`.\n\
                    {}",
                    cycle,
                    max_cycles,
                    goal,
                    AgentToolkit::system_prompt_instructions()
                );

                let user_prompt = format!(
                    "Memoria del Hipocampo 4D:\n{}\n\nEstado Acumulado y Última Observación:\n{}\n\nEjecuta el siguiente paso autónomo del Ciclo {}.",
                    hippo_ctx, rolling_progress, cycle
                );

                let full_chatml = format!(
                    "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                    system_prompt, user_prompt
                );

                let Ok(raw_out) = st.cortex.generate(&full_chatml) else {
                    break;
                };

                let parsed = {
                    let mut tb = st.thought_buffer.lock().await;
                    tb.process_output(&raw_out)
                };

                let tool_calls = AgentToolkit::parse_tool_calls(&parsed.clean_response);
                let mut executed_names = Vec::new();
                let mut observations = String::new();

                for call in &tool_calls {
                    if let AgentToolCall::CreateIsland { name, description } = call {
                        let proto = st.embedder.generate_vector(name).unwrap_or_default();
                        let sum = st.custom_islands.create_island(name, description, &proto, now);
                        executed_names.push(format!("CREATE_ISLAND:{}", sum.name));
                        observations.push_str(&format!("\n[ISLA CREADA]: {}\n", sum.name));
                    } else {
                        let res = AgentToolkit::execute_tool_autonomous(call);
                        executed_names.push(res.tool_name.to_string());
                        observations.push_str(&format!(
                            "\n[OBSERVACIÓN {} (Ciclo {})]:\n{}\n",
                            res.tool_name, cycle, res.output
                        ));
                    }
                }

                // Salvavidas Termodinámico de VRAM: cada 5 ciclos purgamos el KV Cache y cristalizamos un Axioma de Progreso en el Hipocampo 4D
                let mut kv_purged = false;
                if cycle % 5 == 0 {
                    let _ = st.kv_manager.erase_slot(0);
                    kv_purged = true;

                    let axiom_text = format!(
                        "[Axioma de Progreso Maratón - Ciclo {}/{}]: Objetivo: {} | Avance verificado: {}",
                        cycle,
                        max_cycles,
                        goal,
                        parsed.clean_response.chars().take(260).collect::<String>()
                    );
                    if let Ok(ax_vec) = st.embedder.generate_vector(&axiom_text) {
                        let (nid, _) = st.hippocampus.remember(
                            axiom_text.clone(),
                            ax_vec.clone(),
                            Some(IslandCategory::WorkingMemory),
                            2,
                            now,
                        );
                        st.causal_graph.record_validation(nid);
                        st.causal_graph.link_nodes(
                            nid,
                            0,
                            CausalRelation::Optimiza,
                            axiom_text.chars().take(100).collect(),
                            0.95,
                            now,
                        );
                        st.custom_islands.register_base_node(
                            nid,
                            "WorkingMemory",
                            axiom_text,
                            &ax_vec,
                            now,
                        );
                    }
                }

                let short_summary: String = parsed
                    .clean_response
                    .lines()
                    .filter(|l| !l.trim().starts_with("[TOOL:"))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(220)
                    .collect();

                {
                    let mut m = get_marathon_state().lock().await;
                    if kv_purged {
                        m.axioms_crystallized += 1;
                    }
                    m.logs.push(MarathonLogEntry {
                        cycle,
                        timestamp: now,
                        action_summary: if short_summary.is_empty() {
                            format!("Ciclo {} ejecutado con herramientas: {:?}", cycle, executed_names)
                        } else {
                            short_summary.clone()
                        },
                        tools_executed: executed_names,
                        kv_purged,
                    });
                }

                if parsed.clean_response.contains("[MARATHON_COMPLETE]") {
                    let mut m = get_marathon_state().lock().await;
                    m.running = false;
                    break;
                }

                rolling_progress = format!(
                    "Resumen Ciclo {}: {}\nResultados de Herramientas en Ciclo {}:\n{}",
                    cycle,
                    short_summary,
                    cycle,
                    observations.chars().take(1800).collect::<String>()
                );

                tokio::time::sleep( std::time::Duration::from_millis(800)).await;
            }

            let mut m = get_marathon_state().lock().await;
            m.running = false;
        });
    }

    pub async fn stop_marathon() {
        let mut m = get_marathon_state().lock().await;
        m.running = false;
    }
}
