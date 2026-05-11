//! 知识图谱模块
//!
//! 简化的知识图谱实现，支持：
//! - 实体和关系存储
//! - Personalized PageRank 重排序
//! - 子图扩展检索

use std::collections::{HashMap, HashSet};
use tracing::debug;

/// 图节点（实体）
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub name: String,
    pub appearance_count: u32,
}

/// 图边（关系）
#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub weight: f64,
}

/// 知识图谱
#[derive(Debug, Clone, Default)]
pub struct KnowledgeGraph {
    pub nodes: HashMap<String, GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// 邻接表：entity -> [(related_entity, edge_index)]
    adjacency: HashMap<String, Vec<(String, usize)>>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加实体
    pub fn add_entity(&mut self, name: &str) {
        let entry = self.nodes.entry(name.to_lowercase()).or_insert(GraphNode {
            name: name.to_string(),
            appearance_count: 0,
        });
        entry.appearance_count += 1;
    }

    /// 添加关系
    pub fn add_relation(&mut self, subject: &str, predicate: &str, object: &str, weight: f64) {
        // 确保实体存在
        self.add_entity(subject);
        self.add_entity(object);

        let edge_idx = self.edges.len();
        let subject_lower = subject.to_lowercase();
        let object_lower = object.to_lowercase();

        self.edges.push(GraphEdge {
            subject: subject_lower.clone(),
            predicate: predicate.to_string(),
            object: object_lower.clone(),
            weight,
        });

        // 更新邻接表
        self.adjacency
            .entry(subject_lower.clone())
            .or_default()
            .push((object_lower.clone(), edge_idx));
        self.adjacency
            .entry(object_lower)
            .or_default()
            .push((subject_lower, edge_idx));
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
                    for (neighbor, _) in neighbors {
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
    ///
    /// alpha: 重启概率（0.85）
    /// max_iter: 最大迭代次数（100）
    /// tol: 收敛阈值（1e-6）
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

        // 初始化 personalization 向量
        let seed_set: HashSet<String> = seeds.iter().map(|s| s.to_lowercase()).collect();
        let seed_count = seed_set.len().max(1) as f64;
        let p: Vec<f64> = nodes.iter()
            .map(|node| {
                if seed_set.contains(node) { 1.0 / seed_count } else { 0.0 }
            })
            .collect();

        // 初始化 PageRank 向量
        let mut pr: Vec<f64> = vec![1.0 / n as f64; n];

        // 构建转移矩阵（稀疏表示）
        let node_idx: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();

        // 迭代
        for _iter in 0..max_iter {
            let mut new_pr = vec![0.0; n];

            for (i, node) in nodes.iter().enumerate() {
                // 重启项
                new_pr[i] += (1.0 - alpha) * p[i];

                // 从邻居传播
                if let Some(neighbors) = self.adjacency.get(node) {
                    let out_degree = neighbors.len() as f64;
                    if out_degree > 0.0 {
                        for (neighbor, _) in neighbors {
                            if let Some(&j) = node_idx.get(neighbor.as_str()) {
                                new_pr[j] += alpha * pr[i] / out_degree;
                            }
                        }
                    }
                }
            }

            // 检查收敛
            let diff: f64 = pr.iter().zip(new_pr.iter()).map(|(a, b)| (a - b).abs()).sum();
            pr = new_pr;

            if diff < tol {
                debug!(iterations = _iter + 1, diff, "pagerank: converged");
                break;
            }
        }

        // 转换为 HashMap
        nodes.into_iter().zip(pr.into_iter()).collect()
    }

    /// 获取实体的关系
    pub fn get_relations(&self, entity: &str) -> Vec<&GraphEdge> {
        let entity_lower = entity.to_lowercase();
        self.edges.iter()
            .filter(|e| e.subject == entity_lower || e.object == entity_lower)
            .collect()
    }

    /// 获取图谱统计
    pub fn stats(&self) -> (usize, usize) {
        (self.nodes.len(), self.edges.len())
    }
}

/// 从记忆中提取实体和关系（简化版）
///
/// 使用简单的模式匹配提取 (subject, predicate, object) 三元组
pub fn extract_entities_from_text(text: &str) -> Vec<(String, String, String)> {
    let mut triples = Vec::new();

    // 简单的模式匹配：寻找 "A是B"、"A有B"、"A喜欢B" 等模式
    let patterns = [
        ("是", "is"),
        ("有", "has"),
        ("喜欢", "likes"),
        ("属于", "belongs_to"),
        ("包含", "contains"),
        ("位于", "located_at"),
        ("来自", "from"),
        ("使用", "uses"),
        ("知道", "knows"),
    ];

    for (keyword, predicate) in &patterns {
        if let Some(pos) = text.find(keyword) {
            let before = text[..pos].trim();
            let after = text[pos + keyword.len()..].trim();

            // 提取主语和宾语（简化：取关键词前后的词）
            let subject = extract_last_word(before);
            let object = extract_first_word(after);

            if !subject.is_empty() && !object.is_empty() {
                triples.push((subject, predicate.to_string(), object));
            }
        }
    }

    triples
}

/// 提取最后一个词（简化版）
fn extract_last_word(text: &str) -> String {
    text.split_whitespace()
        .last()
        .unwrap_or("")
        .trim_matches(|c: char| c.is_ascii_punctuation())
        .to_string()
}

/// 提取第一个词（简化版）
fn extract_first_word(text: &str) -> String {
    text.split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| c.is_ascii_punctuation())
        .to_string()
}

/// 全局知识图谱
static GRAPH: std::sync::Mutex<Option<KnowledgeGraph>> = std::sync::Mutex::new(None);

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

    with_graph_mut(|graph| {
        for (subject, predicate, object) in triples {
            graph.add_relation(&subject, &predicate, &object, 1.0);
        }
    });

    debug!(user_id, triples = count, "graph: updated from memory");
}
