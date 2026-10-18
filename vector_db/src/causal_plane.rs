use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::common::types::{IslandCategory, NodeId, VectorData, VECTOR_DIM};
use crate::ingestion::normalizer::VectorNormalizer;

pub type Timestamp = u64;

#[inline(always)]
fn dot_product_768(a: &VectorData, b: &VectorData) -> f32 {
    let mut sum = 0.0f32;
    for i in 0..VECTOR_DIM {
        sum += a.raw[i] * b.raw[i];
    }
    sum
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CausalRelation {
    DependeDe,
    FallaCuando,
    Optimiza,
    Implementa,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalEdge {
    pub target_id: NodeId,
    pub relation: CausalRelation,
    pub target_summary: String,
    pub weight: f32,
    pub timestamp: Timestamp,
}

#[derive(Serialize, Deserialize, Default)]
struct CausalSnapshot {
    edges: Vec<(NodeId, Vec<CausalEdge>)>,
    validations: Vec<(NodeId, u32)>,
}

pub struct CausalHypergraph {
    edges: DashMap<NodeId, Vec<CausalEdge>>,
    validations: DashMap<NodeId, u32>,
    base_lambda: f32,
    alpha: f32,
    storage_path: Option<String>,
}

impl CausalHypergraph {
    pub fn new(base_lambda: f32, alpha: f32, storage_path: Option<&str>) -> Self {
        let graph = Self {
            edges: DashMap::new(),
            validations: DashMap::new(),
            base_lambda,
            alpha,
            storage_path: storage_path.map(|s| s.to_string()),
        };
        let _ = graph.load_from_disk();
        graph
    }

    pub fn link_nodes(
        &self,
        source_id: NodeId,
        target_id: NodeId,
        relation: CausalRelation,
        target_summary: String,
        weight: f32,
        now: Timestamp,
    ) {
        if source_id == target_id {
            return;
        }
        let edge = CausalEdge {
            target_id,
            relation,
            target_summary,
            weight,
            timestamp: now,
        };
        let mut entry = self.edges.entry(source_id).or_default();
        if !entry.iter().any(|e| e.target_id == target_id && e.relation == relation) {
            entry.push(edge);
        }
        drop(entry);
        let _ = self.save_to_disk();
    }

    pub fn record_validation(&self, node_id: NodeId) -> u32 {
        let mut count = self.validations.entry(node_id).or_insert(0);
        *count += 1;
        let current = *count;
        drop(count);
        let _ = self.save_to_disk();
        current
    }

    pub fn get_validations(&self, node_id: NodeId) -> u32 {
        self.validations.get(&node_id).map(|v| *v).unwrap_or(1)
    }

    #[inline]
    pub fn effective_lambda(&self, node_id: NodeId, is_core_axiom: bool) -> f32 {
        if is_core_axiom {
            return 0.0;
        }
        let n_val = self.validations.get(&node_id).map(|v| *v as f32).unwrap_or(0.0);
        self.base_lambda / (1.0 + self.alpha * (1.0 + n_val).ln())
    }

    pub fn propagate_from_seeds(
        &self,
        seed_ids: &[NodeId],
        now: Timestamp,
    ) -> Vec<(NodeId, CausalRelation, NodeId, String, f32)> {
        let mut activated = Vec::new();
        for &seed_id in seed_ids {
            if let Some(edge_list) = self.edges.get(&seed_id) {
                let lambda = self.effective_lambda(seed_id, false);
                for edge in edge_list.iter() {
                    let dt = now.saturating_sub(edge.timestamp) as f32;
                    let energy = edge.weight * (-lambda * dt).exp();
                    if energy > 0.15 {
                        activated.push((
                            seed_id,
                            edge.relation,
                            edge.target_id,
                            edge.target_summary.clone(),
                            energy,
                        ));
                    }
                }
            }
        }
        activated
    }

    pub fn export_all_edges(&self) -> Vec<(NodeId, CausalEdge)> {
        let mut all = Vec::new();
        for kv in self.edges.iter() {
            for e in kv.value().iter() {
                all.push((*kv.key(), e.clone()));
            }
        }
        all
    }

    pub fn extract_seed_ids(recall_context: &str) -> Vec<NodeId> {
        let mut ids = Vec::new();
        let marker = "Nodo ID: ";
        let mut search_slice = recall_context;

        while let Some(pos) = search_slice.find(marker) {
            let after = &search_slice[pos + marker.len()..];
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(id) = digits.parse::<NodeId>() {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            search_slice = after;
        }
        ids
    }

    pub fn total_edges(&self) -> usize {
        self.edges.iter().map(|e| e.value().len()).sum()
    }

    pub fn save_to_disk(&self) -> Result<(), String> {
        let path_str = match &self.storage_path {
            Some(p) => p,
            None => return Ok(()),
        };
        let snapshot = CausalSnapshot {
            edges: self.edges.iter().map(|kv| (*kv.key(), kv.value().clone())).collect(),
            validations: self.validations.iter().map(|kv| (*kv.key(), *kv.value())).collect(),
        };
        let file = File::create(path_str).map_err(|e| e.to_string())?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, &snapshot).map_err(|e| e.to_string())
    }

    pub fn load_from_disk(&self) -> Result<usize, String> {
        let path_str = match &self.storage_path {
            Some(p) => p,
            None => return Ok(0),
        };
        if !Path::new(path_str).exists() {
            return Ok(0);
        }
        let file = File::open(path_str).map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);
        let snapshot: CausalSnapshot = bincode::deserialize_from(reader).map_err(|e| e.to_string())?;
        let mut count = 0;
        for (k, v) in snapshot.edges {
            count += v.len();
            self.edges.insert(k, v);
        }
        for (k, v) in snapshot.validations {
            self.validations.insert(k, v);
        }
        Ok(count)
    }

    pub fn random_hypersphere_probe() -> VectorData {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut vec = VectorData::default();
        for i in 0..VECTOR_DIM {
            vec.raw[i] = rng.gen_range(-1.0f32..1.0f32);
        }
        VectorNormalizer::normalize_l2(&mut vec);
        vec
    }

    pub fn extract_nodes_with_text(recall_context: &str) -> Vec<(NodeId, String)> {
        let mut results = Vec::new();
        let marker = "Nodo ID: ";

        for line in recall_context.lines() {
            if let Some(pos) = line.find(marker) {
                let after = &line[pos + marker.len()..];
                let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(id) = digits.parse::<NodeId>() {
                    if let Some(close_paren) = after.find(')') {
                        let text = after[close_paren + 1..].trim().to_string();
                        if text.len() > 25 && !text.contains("/tink") && !results.iter().any(|(existing_id, _)| *existing_id == id) {
                            results.push((id, text));
                        }
                    }
                }
            }
        }
        results
    }
}

pub struct VectorThalamus {
    centroids: RwLock<[(IslandCategory, VectorData); 4]>,
}

impl VectorThalamus {
    pub fn new(
        code_proto: VectorData,
        rules_proto: VectorData,
        working_proto: VectorData,
        episodic_proto: VectorData,
    ) -> Self {
        Self {
            centroids: RwLock::new([
                (IslandCategory::Code, code_proto),
                (IslandCategory::CoreRules, rules_proto),
                (IslandCategory::WorkingMemory, working_proto),
                (IslandCategory::EpisodicMemory, episodic_proto),
            ]),
        }
    }

    #[inline]
    pub fn route_vector(&self, query_vec: &VectorData) -> (IslandCategory, f32) {
        let guards = self.centroids.read();
        let mut best_cat = IslandCategory::WorkingMemory;
        let mut best_sim = f32::MIN;

        for (cat, centroid) in guards.iter() {
            let sim = dot_product_768(query_vec, centroid);
            if sim > best_sim {
                best_sim = sim;
                best_cat = *cat;
            }
        }
        (best_cat, best_sim)
    }

    pub fn reinforce_centroid(&self, category: IslandCategory, sample: &VectorData, lr: f32) {
        let mut guards = self.centroids.write();
        for (cat, centroid) in guards.iter_mut() {
            if *cat == category {
                for i in 0..VECTOR_DIM {
                    centroid.raw[i] = (1.0 - lr) * centroid.raw[i] + lr * sample.raw[i];
                }
                VectorNormalizer::normalize_l2(centroid);
                break;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomIslandNode {
    pub id: NodeId,
    pub text: String,
    pub vector: Vec<f32>,
    pub timestamp: Timestamp,
    pub validations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomIsland {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub centroid: Vec<f32>,
    pub created_at: Timestamp,
    pub nodes: Vec<CustomIslandNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomIslandSummary {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub node_count: usize,
}

/// Estructuras DTO para renderizar el Mapa Sináptico 4D (Estilo Obsidian) en AXIS UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphIslandDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_custom: bool,
    pub node_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNodeDto {
    pub id: String,
    pub raw_id: NodeId,
    pub label: String,
    pub content: String,
    pub island: String,
    pub is_custom: bool,
    pub validations: u32,
    pub lambda: f32,
    pub vector_preview: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdgeDto {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub label: String,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HippocampusGraphSnapshot {
    pub islands: Vec<GraphIslandDto>,
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphEdgeDto>,
    pub total_vectors: usize,
    pub total_synapses: usize,
}

pub struct DynamicIslandManager {
    islands: DashMap<String, CustomIsland>,
    base_nodes: DashMap<NodeId, (String, String, Vec<f32>, Timestamp)>,
    storage_path: Option<String>,
}

impl DynamicIslandManager {
    pub fn new(storage_path: Option<&str>) -> Self {
        let mgr = Self {
            islands: DashMap::new(),
            base_nodes: DashMap::new(),
            storage_path: storage_path.map(|s| s.to_string()),
        };
        let _ = mgr.load_from_disk();
        mgr
    }

    pub fn clean_island_title(raw_name: &str) -> String {
        let stop_words = [
            "dentro", "de", "del", "en", "tu", "mi", "el", "la", "los", "las", "para", "que",
            "con", "sobre", "hipocampo", "cerebro", "donde", "tenga", "guardar", "almacenar",
        ];
        let words: Vec<&str> = raw_name
            .split_whitespace()
            .take_while(|w| {
                let lw = w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
                !stop_words.contains(&lw.as_str())
            })
            .collect();

        let chosen = if words.is_empty() {
            raw_name.split_whitespace().next().unwrap_or("Isla").to_string()
        } else {
            words.join(" ")
        };

        let mut chars = chosen.chars();
        match chars.next() {
            None => "Isla".to_string(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }

    pub fn normalize_slug(name: &str) -> String {
        let clean = Self::clean_island_title(name);
        clean
            .trim()
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .trim_matches('_')
            .to_string()
    }

    /// Registra en memoria un nodo de las 4 islas base para el renderizador visual Obsidian
    pub fn register_base_node(
        &self,
        id: NodeId,
        island_name: &str,
        text: String,
        vec: &VectorData,
        now: Timestamp,
    ) {
        self.base_nodes.insert(
            id,
            (
                island_name.to_string(),
                text,
                vec.raw.to_vec(),
                now,
            ),
        );
    }

    pub fn base_nodes_count(&self) -> usize {
        self.base_nodes.len()
    }

    pub fn create_island(
        &self,
        name: &str,
        description: &str,
        centroid_vec: &VectorData,
        now: Timestamp,
    ) -> CustomIslandSummary {
        let clean_title = Self::clean_island_title(name);
        let slug = Self::normalize_slug(&clean_title);
        let mut entry = self.islands.entry(slug.clone()).or_insert_with(|| CustomIsland {
            slug: slug.clone(),
            name: clean_title.clone(),
            description: description.trim().to_string(),
            centroid: centroid_vec.raw.to_vec(),
            created_at: now,
            nodes: Vec::new(),
        });

        if !description.trim().is_empty() {
            entry.description = description.trim().to_string();
        }
        let summary = CustomIslandSummary {
            slug: entry.slug.clone(),
            name: entry.name.clone(),
            description: entry.description.clone(),
            node_count: entry.nodes.len(),
        };
        drop(entry);
        let _ = self.save_to_disk();
        summary
    }

    pub fn remember_in_island(
        &self,
        island_slug_or_name: &str,
        text: String,
        vec: &VectorData,
        now: Timestamp,
    ) -> Option<(NodeId, String)> {
        let slug = Self::normalize_slug(island_slug_or_name);
        let mut isl = self.islands.get_mut(&slug)?;
        let next_id = (isl.nodes.len() as NodeId) + 1;

        let lr = 0.05f32;
        let mut norm_sq = 0.0f32;
        for i in 0..VECTOR_DIM.min(isl.centroid.len()) {
            isl.centroid[i] = (1.0 - lr) * isl.centroid[i] + lr * vec.raw[i];
            norm_sq += isl.centroid[i] * isl.centroid[i];
        }
        let norm = norm_sq.sqrt().max(1e-12);
        for v in isl.centroid.iter_mut() {
            *v /= norm;
        }

        isl.nodes.push(CustomIslandNode {
            id: next_id,
            text,
            vector: vec.raw.to_vec(),
            timestamp: now,
            validations: 1,
        });

        let name = isl.name.clone();
        drop(isl);
        let _ = self.save_to_disk();
        Some((next_id, name))
    }

    pub fn recall_from_island(
        &self,
        island_slug_or_name: &str,
        query_vec: &VectorData,
        top_k: usize,
        now: Timestamp,
    ) -> Option<(String, f32)> {
        let slug = Self::normalize_slug(island_slug_or_name);
        let isl = self.islands.get(&slug)?;

        let mut centroid_sim = 0.0f32;
        for i in 0..VECTOR_DIM.min(isl.centroid.len()) {
            centroid_sim += query_vec.raw[i] * isl.centroid[i];
        }

        if isl.nodes.is_empty() {
            let header = format!(
                "### [ISLA CORTICAL AISLADA: {} ({})]\n- Dominio: {}\n- Estado: Isla activa (0 recuerdos previos).\n",
                isl.name, isl.slug, isl.description
            );
            return Some((header, centroid_sim.max(0.75)));
        }

        let mut scored: Vec<(f32, f32, &CustomIslandNode)> = isl
            .nodes
            .iter()
            .map(|node| {
                let mut dot = 0.0f32;
                for i in 0..VECTOR_DIM.min(node.vector.len()) {
                    dot += query_vec.raw[i] * node.vector[i];
                }
                let dt = now.saturating_sub(node.timestamp) as f32;
                let lambda = 0.00005 / (1.0 + 2.0 * (1.0 + node.validations as f32).ln());
                let decay = (-lambda * dt).exp();
                let final_score = (dot * 0.85) + (0.15 * decay);
                (dot, final_score, node)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut out = format!(
            "### [MEMORIA AISLADA DE LA ISLA `{}` (Dominio: {}) — {} nodos totales]:\n",
            isl.name,
            isl.description,
            isl.nodes.len()
        );

        let mut max_cos = centroid_sim;
        for (rank, (cos, score, node)) in scored.into_iter().take(top_k).enumerate() {
            if cos > max_cos {
                max_cos = cos;
            }
            out.push_str(&format!(
                "[{}] (Coseno: {:.4}, Score: {:.4}, Isla-Nodo #{}) {}\n",
                rank + 1,
                cos,
                score,
                node.id,
                node.text
            ));
        }

        Some((out, max_cos))
    }

    pub fn list_islands(&self) -> Vec<CustomIslandSummary> {
        let mut list: Vec<CustomIslandSummary> = self
            .islands
            .iter()
            .map(|kv| CustomIslandSummary {
                slug: kv.value().slug.clone(),
                name: kv.value().name.clone(),
                description: kv.value().description.clone(),
                node_count: kv.value().nodes.len(),
            })
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Construye la instantánea completa del Hipocampo 4D para el grafo interactivo estilo Obsidian
    pub fn build_graph_snapshot(&self, causal_graph: &CausalHypergraph) -> HippocampusGraphSnapshot {
        let mut islands = vec![
            GraphIslandDto {
                id: "CoreRules".to_string(),
                name: "CoreRules (Axiomas)".to_string(),
                description: "Reglas base de hardware, topología LEIKA y axiomas inquebrantables (λ → 0)".to_string(),
                is_custom: false,
                node_count: 0,
            },
            GraphIslandDto {
                id: "Code".to_string(),
                name: "Code (Ingeniería)".to_string(),
                description: "Repositorios locales de Daniel, arquitecturas Rust/Tauri y victorias Hyper-REM".to_string(),
                is_custom: false,
                node_count: 0,
            },
            GraphIslandDto {
                id: "WorkingMemory".to_string(),
                name: "WorkingMemory".to_string(),
                description: "Contexto táctico de corto plazo y telemetría operativa activa".to_string(),
                is_custom: false,
                node_count: 0,
            },
            GraphIslandDto {
                id: "EpisodicMemory".to_string(),
                name: "EpisodicMemory".to_string(),
                description: "Historial conversacional y trazas de sesiones previas".to_string(),
                is_custom: false,
                node_count: 0,
            },
        ];

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut all_vectors: Vec<(String, String, Vec<f32>)> = Vec::new();

        // 1. Nodos de las 4 Islas Base
        let mut sorted_base: Vec<_> = self
            .base_nodes
            .iter()
            .map(|kv| (*kv.key(), kv.value().clone()))
            .collect();
        sorted_base.sort_by_key(|(id, _)| *id);

        for (raw_id, (isl_name, text, vec, _ts)) in sorted_base {
            let node_uid = format!("base-{}", raw_id);
            let clean_label: String = text
                .replace("Proyecto Local Indexado ", "Repo ")
                .replace("Axioma ", "")
                .replace("[Trayectoria Ideal ABE /deep-think]: Problema: ", "ABE: ")
                .replace("[Axioma Cristalizado Hyper-REM]: ", "REM: ")
                .replace("Usuario (AXIS):", "")
                .replace("Usuario:", "")
                .replace("Daniel:", "")
                .replace('`', "")
                .trim()
                .chars()
                .take(34)
                .collect();

            let vals = causal_graph.get_validations(raw_id);
            let is_core = isl_name == "CoreRules";
            let lambda = causal_graph.effective_lambda(raw_id, is_core);
            let preview: Vec<f32> = vec.iter().copied().take(48).collect();

            if let Some(isl_dto) = islands.iter_mut().find(|i| i.id == isl_name) {
                isl_dto.node_count += 1;
            }

            nodes.push(GraphNodeDto {
                id: node_uid.clone(),
                raw_id,
                label: format!("#{} {}", raw_id, clean_label),
                content: text,
                island: isl_name.clone(),
                is_custom: false,
                validations: vals,
                lambda,
                vector_preview: preview,
            });

            edges.push(GraphEdgeDto {
                source: node_uid.clone(),
                target: format!("island-{}", isl_name),
                kind: "island_member".to_string(),
                label: "PerteneceA".to_string(),
                weight: 0.8,
            });

            all_vectors.push((node_uid, isl_name, vec));
        }

        // 2. Islas Personalizadas y sus Nodos Aislados
        for kv in self.islands.iter() {
            let isl = kv.value();
            let isl_id = format!("custom:{}", isl.slug);
            islands.push(GraphIslandDto {
                id: isl_id.clone(),
                name: format!("Isla: {}", isl.name),
                description: isl.description.clone(),
                is_custom: true,
                node_count: isl.nodes.len(),
            });

            for cnode in &isl.nodes {
                let node_uid = format!("custom-{}-{}", isl.slug, cnode.id);
                let clean_label: String = cnode
                    .text
                    .replace("Daniel:", "")
                    .trim()
                    .chars()
                    .take(44)
                    .collect();
                let lambda = 0.00005 / (1.0 + 2.0 * (1.0 + cnode.validations as f32).ln());
                let preview: Vec<f32> = cnode.vector.iter().copied().take(48).collect();

                nodes.push(GraphNodeDto {
                    id: node_uid.clone(),
                    raw_id: cnode.id,
                    label: format!("[{}] #{} {}", isl.name, cnode.id, clean_label),
                    content: cnode.text.clone(),
                    island: isl_id.clone(),
                    is_custom: true,
                    validations: cnode.validations,
                    lambda,
                    vector_preview: preview,
                });

                edges.push(GraphEdgeDto {
                    source: node_uid.clone(),
                    target: format!("island-{}", isl_id),
                    kind: "island_member".to_string(),
                    label: "ConfinadoEn".to_string(),
                    weight: 0.95,
                });

                all_vectors.push((node_uid, isl_id.clone(), cnode.vector.clone()));
            }
        }

        // 3. Aristas Causales Dirigidas del Plano 2 (bob_causal.bin)
        for (src_raw, c_edge) in causal_graph.export_all_edges() {
            let src_uid = format!("base-{}", src_raw);
            let dst_uid = format!("base-{}", c_edge.target_id);
            if nodes.iter().any(|n| n.id == src_uid) && nodes.iter().any(|n| n.id == dst_uid) {
                edges.push(GraphEdgeDto {
                    source: src_uid,
                    target: dst_uid,
                    kind: "causal".to_string(),
                    label: format!("{:?}", c_edge.relation),
                    weight: c_edge.weight,
                });
            }
        }

        // 4. Puentes de Similitud Coseno K-NN (Top-2 vecinos por nodo para evitar saturación visual)
        for i in 0..all_vectors.len() {
            let (ref id_a, ref isl_a, ref vec_a) = all_vectors[i];
            let mut neighbors: Vec<(f32, String)> = Vec::new();

            for j in (i + 1)..all_vectors.len() {
                let (ref id_b, ref isl_b, ref vec_b) = all_vectors[j];
                if (isl_a.starts_with("custom:") || isl_b.starts_with("custom:")) && isl_a != isl_b {
                    continue;
                }
                let mut dot = 0.0f32;
                let len = vec_a.len().min(vec_b.len()).min(VECTOR_DIM);
                for k in 0..len {
                    dot += vec_a[k] * vec_b[k];
                }
                if dot >= 0.76 {
                    neighbors.push((dot, id_b.clone()));
                }
            }

            neighbors.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            for (dot, target_id) in neighbors.into_iter().take(2) {
                edges.push(GraphEdgeDto {
                    source: id_a.clone(),
                    target: target_id,
                    kind: "semantic".to_string(),
                    label: format!("cos={:.2}", dot),
                    weight: dot,
                });
            }
        }

        let total_vectors = nodes.len();
        let total_synapses = edges.len();

        HippocampusGraphSnapshot {
            islands,
            nodes,
            edges,
            total_vectors,
            total_synapses,
        }
    }

    pub fn save_to_disk(&self) -> Result<(), String> {
        let Some(path) = &self.storage_path else {
            return Ok(());
        };
        let all: Vec<CustomIsland> = self.islands.iter().map(|kv| kv.value().clone()).collect();
        let file = File::create(path).map_err(|e| e.to_string())?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, &all).map_err(|e| e.to_string())
    }

    pub fn load_from_disk(&self) -> Result<usize, String> {
        let Some(path) = &self.storage_path else {
            return Ok(0);
        };
        if !Path::new(path).exists() {
            return Ok(0);
        }
        let file = File::open(path).map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);
        let all: Vec<CustomIsland> = bincode::deserialize_from(reader).map_err(|e| e.to_string())?;
        let mut migrated = false;

        for mut isl in all {
            // Auto-limpiar el nombre si tenía sufijos como "ventas_dentro_de" -> "ventas"
            let clean_title = Self::clean_island_title(&isl.name);
            let clean_slug = Self::normalize_slug(&clean_title);
            if clean_slug != isl.slug {
                isl.slug = clean_slug;
                isl.name = clean_title;
                migrated = true;
            }
            self.islands.insert(isl.slug.clone(), isl);
        }

        if migrated {
            let _ = self.save_to_disk();
        }
        Ok(self.islands.len())
    }
}
