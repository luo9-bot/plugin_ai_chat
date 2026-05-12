//! 知识图谱模块
//!
//! 有向图 + 边属性存储，支持：
//! - 实体和关系存储（有向图）
//! - 关系属性（权重、时间、来源、证据等）
//! - Aho-Corasick + LLM 实体提取
//! - Personalized PageRank 重排序
//! - BFS 子图扩展
//! - 关系向量检索

use std::collections::{HashMap, HashSet};
use tracing::debug;

/// 图节点（实体）
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub name: String,
    pub appearance_count: u32,
    /// 实体类型（person/place/thing/concept等）
    pub entity_type: String,
}

/// 图边（关系）- 有向边
#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub weight: f64,
    /// 关系可信度 (0.0~1.0)
    pub confidence: f64,
    /// 首次记录时间
    pub created_at: u64,
    /// 最后更新时间
    pub updated_at: u64,
    /// 出现次数
    pub count: u32,
    /// 来源（记忆内容摘要）
    pub source: String,
    /// 可选的边嵌入向量哈希（用于关联向量存储）
    pub vector_hash: Option<String>,
}

/// 知识图谱
#[derive(Debug, Clone, Default)]
pub struct KnowledgeGraph {
    pub nodes: HashMap<String, GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// 邻接表：entity -> [(related_entity, edge_index, is_outgoing)]
    adjacency: HashMap<String, Vec<(String, usize, bool)>>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加实体
    pub fn add_entity(&mut self, name: &str, entity_type: &str) {
        let entry = self.nodes.entry(name.to_lowercase()).or_insert(GraphNode {
            name: name.to_string(),
            appearance_count: 0,
            entity_type: entity_type.to_string(),
        });
        entry.appearance_count += 1;
    }

    /// 添加有向关系
    #[allow(clippy::too_many_arguments)]
    pub fn add_relation(
        &mut self,
        subject: &str,
        predicate: &str,
        object: &str,
        weight: f64,
        confidence: f64,
        source: &str,
        now: u64,
    ) {
        self.add_entity(subject, "entity");
        self.add_entity(object, "entity");

        let edge_idx = self.edges.len();
        let subject_lower = subject.to_lowercase();
        let object_lower = object.to_lowercase();

        self.edges.push(GraphEdge {
            subject: subject_lower.clone(),
            predicate: predicate.to_string(),
            object: object_lower.clone(),
            weight,
            confidence,
            created_at: now,
            updated_at: now,
            count: 1,
            source: source.to_string(),
            vector_hash: None,
        });

        // 有向边：出边
        self.adjacency
            .entry(subject_lower.clone())
            .or_default()
            .push((object_lower.clone(), edge_idx, true));
        // 反向边：入边（用于PageRank反向传播）
        self.adjacency
            .entry(object_lower)
            .or_default()
            .push((subject_lower, edge_idx, false));
    }

    /// 合并或更新关系（如果已存在则增加权重和计数）
    #[allow(clippy::too_many_arguments)]
    pub fn merge_relation(
        &mut self,
        subject: &str,
        predicate: &str,
        object: &str,
        weight: f64,
        confidence: f64,
        source: &str,
        now: u64,
    ) {
        let subject_lower = subject.to_lowercase();
        let object_lower = object.to_lowercase();

        for edge in &mut self.edges {
            if edge.subject == subject_lower
                && edge.predicate == predicate
                && edge.object == object_lower
            {
                edge.weight = edge.weight * 0.7 + weight * 0.3;
                edge.confidence = edge.confidence.max(confidence);
                edge.count += 1;
                edge.updated_at = now;
                if !source.is_empty() {
                    edge.source = source.to_string();
                }
                return;
            }
        }
        self.add_relation(subject, predicate, object, weight, confidence, source, now);
    }

    /// BFS 子图扩展：从种子实体出发，扩展 depth 层
    pub fn expand_subgraph(&self, seeds: &[String], depth: usize) -> HashSet<String> {
        let mut visited = HashSet::new();
        let mut current_layer: HashSet<String> = seeds.iter().map(|s| s.to_lowercase()).collect();

        for _ in 0..depth {
            let mut next_layer = HashSet::new();
            for entity in &current_layer {
                if visited.contains(entity) {
                    continue;
                }
                visited.insert(entity.clone());
                if let Some(neighbors) = self.adjacency.get(entity) {
                    for (neighbor, _, _) in neighbors {
                        if !visited.contains(neighbor) {
                            next_layer.insert(neighbor.clone());
                        }
                    }
                }
            }
            current_layer = next_layer;
        }
        visited
    }

    /// Personalized PageRank
    pub fn personalized_pagerank(
        &self,
        seeds: &[String],
        alpha: f64,
        max_iter: usize,
        tol: f64,
    ) -> HashMap<String, f64> {
        let nodes: Vec<String> = self.nodes.keys().cloned().collect();
        let n = nodes.len();
        if n == 0 {
            return HashMap::new();
        }

        let seed_set: HashSet<String> = seeds.iter().map(|s| s.to_lowercase()).collect();
        let seed_count = seed_set.len().max(1) as f64;
        let p: Vec<f64> = nodes
            .iter()
            .map(|node| {
                if seed_set.contains(node) {
                    1.0 / seed_count
                } else {
                    0.0
                }
            })
            .collect();

        let mut pr: Vec<f64> = vec![1.0 / n as f64; n];
        let node_idx: HashMap<&str, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();

        for _iter in 0..max_iter {
            let mut new_pr = vec![0.0; n];
            for (i, node) in nodes.iter().enumerate() {
                new_pr[i] += (1.0 - alpha) * p[i];
                if let Some(neighbors) = self.adjacency.get(node) {
                    let out_degree = neighbors.len() as f64;
                    if out_degree > 0.0 {
                        for (neighbor, _, _) in neighbors {
                            if let Some(&j) = node_idx.get(neighbor.as_str()) {
                                new_pr[j] += alpha * pr[i] / out_degree;
                            }
                        }
                    }
                }
            }
            let diff: f64 = pr
                .iter()
                .zip(new_pr.iter())
                .map(|(a, b)| (a - b).abs())
                .sum();
            pr = new_pr;
            if diff < tol {
                debug!(iterations = _iter + 1, diff, "pagerank: converged");
                break;
            }
        }

        nodes.into_iter().zip(pr).collect()
    }

    /// 从边权重计算 Personalized PageRank（边权重感知）
    pub fn weighted_pagerank(
        &self,
        seeds: &[String],
        alpha: f64,
        max_iter: usize,
        tol: f64,
    ) -> HashMap<String, f64> {
        let nodes: Vec<String> = self.nodes.keys().cloned().collect();
        let n = nodes.len();
        if n == 0 {
            return HashMap::new();
        }

        let seed_set: HashSet<String> = seeds.iter().map(|s| s.to_lowercase()).collect();
        let seed_count = seed_set.len().max(1) as f64;
        let p: Vec<f64> = nodes
            .iter()
            .map(|node| {
                if seed_set.contains(node) {
                    1.0 / seed_count
                } else {
                    0.0
                }
            })
            .collect();

        let mut pr: Vec<f64> = vec![1.0 / n as f64; n];
        let node_idx: HashMap<&str, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();

        for _iter in 0..max_iter {
            let mut new_pr = vec![0.0; n];
            for (i, node) in nodes.iter().enumerate() {
                new_pr[i] += (1.0 - alpha) * p[i];
                if let Some(neighbors) = self.adjacency.get(node) {
                    let total_weight: f64 = neighbors
                        .iter()
                        .filter_map(|(_, edge_idx, _)| self.edges.get(*edge_idx))
                        .map(|e| e.weight)
                        .sum();
                    if total_weight > 0.0 {
                        for (neighbor, edge_idx, _) in neighbors {
                            if let Some(edge) = self.edges.get(*edge_idx) {
                                let w = edge.weight / total_weight;
                                if let Some(&j) = node_idx.get(neighbor.as_str()) {
                                    new_pr[j] += alpha * pr[i] * w;
                                }
                            }
                        }
                    }
                }
            }
            let diff: f64 = pr
                .iter()
                .zip(new_pr.iter())
                .map(|(a, b)| (a - b).abs())
                .sum();
            pr = new_pr;
            if diff < tol {
                break;
            }
        }
        nodes.into_iter().zip(pr).collect()
    }

    /// 获取实体的出边关系
    pub fn get_outgoing_relations(&self, entity: &str) -> Vec<&GraphEdge> {
        let entity_lower = entity.to_lowercase();
        self.edges
            .iter()
            .filter(|e| e.subject == entity_lower)
            .collect()
    }

    /// 获取实体的入边关系
    pub fn get_incoming_relations(&self, entity: &str) -> Vec<&GraphEdge> {
        let entity_lower = entity.to_lowercase();
        self.edges
            .iter()
            .filter(|e| e.object == entity_lower)
            .collect()
    }

    /// 获取实体的所有关系
    pub fn get_relations(&self, entity: &str) -> Vec<&GraphEdge> {
        let entity_lower = entity.to_lowercase();
        self.edges
            .iter()
            .filter(|e| e.subject == entity_lower || e.object == entity_lower)
            .collect()
    }

    /// 获取图谱统计
    pub fn stats(&self) -> (usize, usize) {
        (self.nodes.len(), self.edges.len())
    }
}

/// Aho-Corasick 构建的实体匹配器，用于从文本中快速匹配已知实体
pub struct EntityMatcher {
    ac: aho_corasick::AhoCorasick,
    entities: Vec<String>,
}

impl EntityMatcher {
    pub fn build(entities: &[String]) -> Self {
        let ac = aho_corasick::AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build(entities)
            .unwrap_or_else(|_| aho_corasick::AhoCorasick::new(&[] as &[String]).unwrap());
        Self {
            ac,
            entities: entities.to_vec(),
        }
    }

    /// 在文本中匹配已知实体
    pub fn match_entities(&self, text: &str) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        for m in self.ac.find_iter(text) {
            if let Some(name) = self.entities.get(m.pattern().as_usize())
                && !found.contains(name) {
                    found.push(name.clone());
                }
        }
        found
    }
}

/// 从记忆中提取实体和关系（规则 + 模式匹配）
pub fn extract_entities_from_text(text: &str) -> Vec<(String, String, String)> {
    let mut triples = Vec::new();

    // 中文关系模式
    let patterns = [
        ("是", "is"),
        ("叫", "is"),
        ("有", "has"),
        ("喜欢", "likes"),
        ("不喜欢", "dislikes"),
        ("讨厌", "hates"),
        ("属于", "belongs_to"),
        ("包含", "contains"),
        ("位于", "located_at"),
        ("来自", "from"),
        ("使用", "uses"),
        ("知道", "knows"),
        ("住在", "lives_in"),
        ("在", "at"),
        ("的", "possesses"),
    ];

    for (keyword, predicate) in &patterns {
        let mut start = 0;
        while let Some(pos) = text[start..].find(keyword) {
            let abs_pos = start + pos;
            let before = text[..abs_pos].trim();
            let after = text[abs_pos + keyword.len()..].trim();

            let subject = extract_last_word(before);
            let object = extract_first_word(after);

            if !subject.is_empty() && !object.is_empty() {
                triples.push((subject, predicate.to_string(), object));
            }
            start = abs_pos + 1;
        }
    }

    triples
}

fn extract_last_word(text: &str) -> String {
    text.split_whitespace()
        .last()
        .unwrap_or("")
        .trim_matches(|c: char| c.is_ascii_punctuation() || c == '，' || c == '。')
        .to_string()
}

fn extract_first_word(text: &str) -> String {
    text.split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| c.is_ascii_punctuation() || c == '，' || c == '。')
        .to_string()
}

// ── 全局知识图谱 ────────────────────────────────────────────────

use std::sync::Mutex;

static GRAPH: Mutex<Option<KnowledgeGraph>> = Mutex::new(None);

/// 初始化知识图谱
pub fn init() {
    let mut guard = GRAPH.lock().unwrap();
    *guard = Some(KnowledgeGraph::new());
}

/// 获取知识图谱引用
pub fn with_graph<F, R>(f: F) -> R
where
    F: FnOnce(&KnowledgeGraph) -> R,
{
    let guard = GRAPH.lock().unwrap();
    let graph = guard.as_ref().expect("KnowledgeGraph not initialized");
    f(graph)
}

/// 获取可变知识图谱引用
pub fn with_graph_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut KnowledgeGraph) -> R,
{
    let mut guard = GRAPH.lock().unwrap();
    let graph = guard.as_mut().expect("KnowledgeGraph not initialized");
    f(graph)
}

/// 从记忆中提取实体关系并更新图谱
pub fn update_graph_from_memory(user_id: u64, content: &str) {
    let triples = extract_entities_from_text(content);
    let count = triples.len();
    if triples.is_empty() {
        return;
    }

    let now = crate::util::now_secs();
    with_graph_mut(|graph| {
        for (subject, predicate, object) in triples {
            graph.merge_relation(&subject, &predicate, &object, 1.0, 0.8, content, now);
        }
    });

    debug!(user_id, triples = count, "graph: updated from memory");
}

/// 构建全局实体匹配器
pub fn build_entity_matcher() -> EntityMatcher {
    with_graph(|graph| {
        let entities: Vec<String> = graph.nodes.keys().cloned().collect();
        EntityMatcher::build(&entities)
    })
}
