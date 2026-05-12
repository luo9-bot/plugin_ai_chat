//! 智能路径回退
//!
//! 当检索结果不足时，自动尝试回退策略：
//! 1. 查询简化：去除停用词、保留关键词
//! 2. 语义扩展：使用同义词/近义词扩展
//! 3. 图扩展：沿知识图谱向外扩展一层

use std::collections::HashSet;

/// 智能回退配置
pub struct FallbackConfig {
    /// 最少结果数，低于此值触发回退
    pub min_results: usize,
    /// 是否启用查询简化
    pub enable_query_simplify: bool,
    /// 是否启用图扩展
    pub enable_graph_expansion: bool,
    /// 图扩展层数
    pub graph_expansion_depth: usize,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            min_results: 3,
            enable_query_simplify: true,
            enable_graph_expansion: true,
            graph_expansion_depth: 1,
        }
    }
}

/// 中文停用词（精简版）
const STOP_WORDS: &[&str] = &[
    "的", "了", "在", "是", "我", "有", "和", "就", "不", "人", "都", "一",
    "一个", "上", "也", "很", "到", "说", "要", "去", "你", "会", "着",
    "没有", "看", "好", "自己", "这", "他", "她", "它", "们", "那", "什么",
    "怎么", "如何", "哪个", "为什么", "啥", "吗", "吧", "呢", "啊", "嗯",
    "请问", "请", "帮", "能", "可以", "应该", "需要", "想",
];

/// 查询简化：移除停用词，保留关键词
pub fn simplify_query(query: &str) -> String {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    let keywords: Vec<&str> = tokens
        .into_iter()
        .filter(|t| !STOP_WORDS.contains(t) && t.len() > 1)
        .collect();

    if keywords.is_empty() {
        // 如果全部是停用词，保留原查询
        query.to_string()
    } else {
        keywords.join(" ")
    }
}

/// 图扩展：从种子实体出发，获取相关实体
pub fn expand_with_graph(seeds: &[String], depth: usize) -> Vec<String> {
    crate::memory::graph::with_graph(|graph| {
        let expanded = graph.expand_subgraph(seeds, depth);
        let mut result: Vec<String> = expanded.into_iter().collect();
        // 种子实体排在前面
        let seed_set: HashSet<String> = seeds.iter().map(|s| s.to_lowercase()).collect();
        result.sort_by(|a, b| {
            let a_is_seed = seed_set.contains(a);
            let b_is_seed = seed_set.contains(b);
            if a_is_seed && !b_is_seed {
                std::cmp::Ordering::Less
            } else if !a_is_seed && b_is_seed {
                std::cmp::Ordering::Greater
            } else {
                a.cmp(b)
            }
        });
        result
    })
}

/// 执行智能回退：按策略依次尝试
pub fn smart_fallback(
    query: &str,
    current_count: usize,
    config: &FallbackConfig,
) -> Vec<String> {
    let mut fallback_queries = Vec::new();

    if current_count >= config.min_results {
        return fallback_queries;
    }

    // 策略1: 查询简化
    if config.enable_query_simplify {
        let simplified = simplify_query(query);
        if simplified != query && !simplified.is_empty() {
            fallback_queries.push(simplified);
        }
    }

    // 策略2: 图扩展（从查询中提取可能的实体）
    if config.enable_graph_expansion {
        let seeds: Vec<String> = query
            .split_whitespace()
            .filter(|w| w.len() >= 2)
            .map(|w| w.to_string())
            .collect();
        if !seeds.is_empty() {
            let expanded = expand_with_graph(&seeds, config.graph_expansion_depth);
            for entity in expanded {
                let eq = format!("{} {}", query, entity);
                if !fallback_queries.contains(&eq) {
                    fallback_queries.push(eq);
                }
            }
        }
    }

    fallback_queries
}
