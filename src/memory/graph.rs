//! 知识图谱模块
//!
//! 有向图 + 边属性存储，支持：
//! - 实体和关系存储（有向图）
//! - 关系属性（权重、时间、来源、证据等）
//! - Aho-Corasick + LLM 实体提取
//! - Personalized PageRank 重排序
//! - BFS 子图扩展
//! - 关系向量检索

use std::collections::HashMap;
use tracing::debug;

/// 图节点（实体）
#[derive(Debug, Clone)]
pub(crate) struct GraphNode {
    pub appearance_count: u32,
}

/// 图边（关系）- 有向边
#[derive(Debug, Clone)]
pub(crate) struct GraphEdge {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub weight: f64,
    /// 关系可信度 (0.0~1.0)
    pub confidence: f64,
    /// 最后更新时间
    pub updated_at: u64,
    /// 出现次数
    pub count: u32,
    /// 来源（记忆内容摘要）
    pub source: String,
}

/// 知识图谱
#[derive(Debug, Clone, Default)]
pub(crate) struct KnowledgeGraph {
    pub nodes: HashMap<String, GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// 邻接表：entity -> [(related_entity, edge_index, is_outgoing)]
    adjacency: HashMap<String, Vec<(String, usize, bool)>>,
}

impl KnowledgeGraph {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 添加实体（节点以名称小写为 key，因此节点本身不再存一份名字）
    pub(crate) fn add_entity(&mut self, name: &str) {
        let entry = self.nodes.entry(name.to_lowercase()).or_insert(GraphNode {
            appearance_count: 0,
        });
        entry.appearance_count += 1;
    }

    /// 添加有向关系
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_relation(
        &mut self,
        subject: &str,
        predicate: &str,
        object: &str,
        weight: f64,
        confidence: f64,
        source: &str,
        now: u64,
    ) {
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
            confidence,
            updated_at: now,
            count: 1,
            source: source.to_string(),
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
    pub(crate) fn merge_relation(
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
}

/// Aho-Corasick 构建的实体匹配器，用于从文本中快速匹配已知实体
pub(crate) struct EntityMatcher {
    /// 构建失败时为 `None`（例如模式集合为空）：那时"匹配不到实体"，
    /// 而不是让调用方 panic
    ac: Option<aho_corasick::AhoCorasick>,
    entities: Vec<String>,
}

impl EntityMatcher {
    pub(crate) fn build(entities: &[String]) -> Self {
        let ac = aho_corasick::AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build(entities)
            .ok();
        if ac.is_none() {
            tracing::debug!("graph: 实体匹配器构建失败，本次不做实体匹配");
        }
        Self {
            ac,
            entities: entities.to_vec(),
        }
    }

    /// 在文本中匹配已知实体
    pub(crate) fn match_entities(&self, text: &str) -> Vec<String> {
        let Some(ac) = &self.ac else {
            return Vec::new();
        };
        let mut found: Vec<String> = Vec::new();
        for m in ac.find_iter(text) {
            if let Some(name) = self.entities.get(m.pattern().as_usize())
                && !found.contains(name)
            {
                found.push(name.clone());
            }
        }
        found
    }
}

/// 从记忆中提取实体和关系（规则 + 模式匹配）
pub(crate) fn extract_entities_from_text(text: &str) -> Vec<(String, String, String)> {
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
            // 推进到下一个 char 边界，避免多字节 UTF-8 字符中间切片
            start = abs_pos + keyword.len();
            while start < text.len() && !text.is_char_boundary(start) {
                start += 1;
            }
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

use crate::util::MutexExt;
use std::sync::Mutex;

static GRAPH: Mutex<Option<KnowledgeGraph>> = Mutex::new(None);

/// 初始化知识图谱
pub(crate) fn init() {
    let mut guard = GRAPH.lock_recover();
    *guard = Some(KnowledgeGraph::new());
}

/// 获取知识图谱引用
///
/// 未初始化时惰性建一个空图：知识图谱是纯内存的加速结构，
/// "还没初始化"不该让检索路径 panic（原先这里是 `expect`）。
pub(crate) fn with_graph<F, R>(f: F) -> R
where
    F: FnOnce(&KnowledgeGraph) -> R,
{
    let mut guard = GRAPH.lock_recover();
    f(guard.get_or_insert_with(KnowledgeGraph::new))
}

/// 获取可变知识图谱引用（同 [`with_graph`]，未初始化时惰性建空图）
pub(crate) fn with_graph_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut KnowledgeGraph) -> R,
{
    let mut guard = GRAPH.lock_recover();
    f(guard.get_or_insert_with(KnowledgeGraph::new))
}

/// 从记忆中提取实体关系并更新图谱
pub(crate) fn update_graph_from_memory(user_id: u64, content: &str) {
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
pub(crate) fn build_entity_matcher() -> EntityMatcher {
    with_graph(|graph| {
        let entities: Vec<String> = graph.nodes.keys().cloned().collect();
        EntityMatcher::build(&entities)
    })
}
