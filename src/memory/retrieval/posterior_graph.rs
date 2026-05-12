//! 后置图门控 (Posterior Graph Gate)
//!
//! 在检索结果的后处理阶段应用图结构做二次过滤：
//! 1. 检查结果之间的图连通性
//! 2. 提高连通子图内结果的分数
//! 3. 降低孤立结果的分数
//! 4. 保持最终结果多样性

use std::collections::{HashMap, HashSet};

use super::fusion::RetrievalResult;

/// 后置图门控配置
pub struct PosteriorGraphConfig {
    pub enabled: bool,
    /// 丢弃比例（丢弃分数最低的多少结果）
    pub drop_ratio: f64,
    /// 最少核心结果数
    pub min_core_results: usize,
    /// 最多图补充结果数
    pub max_graph_slots: usize,
    /// 连通性提升权重
    pub connectivity_boost: f64,
    /// 孤立结果惩罚
    pub isolation_penalty: f64,
}

impl Default for PosteriorGraphConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            drop_ratio: 0.15,
            min_core_results: 2,
            max_graph_slots: 2,
            connectivity_boost: 0.15,
            isolation_penalty: 0.3,
        }
    }
}

/// 应用后置图门控
pub fn apply_posterior_graph_gate(
    results: &mut Vec<RetrievalResult>,
    config: &PosteriorGraphConfig,
) {
    if !config.enabled || results.len() < config.min_core_results {
        return;
    }

    // 从结果中提取实体
    let result_entities: Vec<HashSet<String>> = results
        .iter()
        .map(|r| extract_entities(&r.content))
        .collect();

    // 计算实体间的图连通性
    let connectivity = compute_connectivity(&result_entities);

    // 调整分数
    for (i, result) in results.iter_mut().enumerate() {
        let conn_score = connectivity.get(&i).copied().unwrap_or(0.0);
        if conn_score > 0.0 {
            result.score *= 1.0 + config.connectivity_boost * conn_score;
        } else {
            result.score *= 1.0 - config.isolation_penalty;
        }
    }

    // 重新排序
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // 丢弃分数最低的一部分结果
    let drop_count = (results.len() as f64 * config.drop_ratio).round() as usize;
    if drop_count > 0 && results.len() > config.min_core_results {
        results.truncate(results.len() - drop_count);
    }
}

/// 从文本中提取实体（使用图谱匹配器）
fn extract_entities(text: &str) -> HashSet<String> {
    let matcher = crate::memory::graph::build_entity_matcher();
    let matched = matcher.match_entities(text);
    matched.into_iter().collect()
}

/// 计算结果之间的图连通性矩阵
///
/// 返回每个结果与其他结果的连通度（0.0~1.0）
fn compute_connectivity(result_entities: &[HashSet<String>]) -> HashMap<usize, f64> {
    let mut connectivity = HashMap::new();

    for i in 0..result_entities.len() {
        let mut connected_count = 0;
        for j in 0..result_entities.len() {
            if i == j {
                continue;
            }
            // 两个结果共享实体即为连通
            if result_entities[i].iter().any(|e| result_entities[j].contains(e)) {
                connected_count += 1;
            }
        }
        let score = if result_entities.len() > 1 {
            connected_count as f64 / (result_entities.len() - 1) as f64
        } else {
            0.0
        };
        connectivity.insert(i, score);
    }

    connectivity
}
