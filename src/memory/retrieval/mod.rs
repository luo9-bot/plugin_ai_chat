//! 记忆检索模块
//!
//! 双路检索：向量语义搜索 + BM25 关键词搜索
//! 使用 Weighted RRF 融合两路结果
//! 支持后置图门控、自适应阈值

pub mod bm25;
#[cfg(test)]
mod eval;
pub mod forgetting;
mod fusion;
pub mod posterior_graph;
pub mod threshold;
pub(crate) mod vector;

pub use fusion::RetrievalResult;
pub use posterior_graph::PosteriorGraphConfig;
pub use threshold::ThresholdConfig;

use std::collections::HashMap;

/// 双路检索配置
pub(crate) struct RetrievalConfig {
    pub top_k: usize,
    pub vector_weight: f64,
    pub bm25_weight: f64,
    pub rrf_k: f64,
    pub threshold_config: Option<ThresholdConfig>,
    pub posterior_graph_config: Option<PosteriorGraphConfig>,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: 10,
            vector_weight: 0.7,
            bm25_weight: 0.3,
            rrf_k: 60.0,
            threshold_config: None,
            posterior_graph_config: None,
        }
    }
}

/// 双路检索（完整版）
///
/// 1. BM25 关键词检索
/// 2. 向量语义检索
/// 3. Weighted RRF 融合
/// 4. 后置图门控
/// 5. 自适应阈值过滤
pub(crate) fn dual_path_retrieve(
    query: &str,
    memories: &[(String, String)],     // (id, content)
    embeddings: &[(String, Vec<f32>)], // (id, embedding)
    config: &RetrievalConfig,
) -> Vec<RetrievalResult> {
    if memories.is_empty() {
        return Vec::new();
    }

    // 步骤1: BM25 关键词检索
    let bm25_results = bm25::search(query, memories, config.top_k * 2);

    // 步骤2: 向量语义检索（只使用缓存的查询向量，不阻塞调用 API）
    let vector_results = if let Some(query_embedding) = vector::get_cached_query_embedding(query) {
        vector::search(&query_embedding, embeddings, config.top_k * 2)
    } else {
        // 没有缓存的查询向量，跳过向量检索，纯 BM25 结果
        Vec::new()
    };

    // 步骤3: Weighted RRF 融合
    let mut fused = fusion::weighted_rrf_fusion(
        &vector_results,
        &bm25_results,
        config.rrf_k,
        config.vector_weight,
        config.bm25_weight,
    );

    // 填充 content
    let doc_map: HashMap<String, String> = memories.iter().cloned().collect();
    for result in &mut fused {
        if let Some(content) = doc_map.get(&result.id) {
            result.content = content.clone();
        }
    }

    // 步骤4: 后置图门控
    if let Some(ref pg_config) = config.posterior_graph_config {
        posterior_graph::apply_posterior_graph_gate(&mut fused, pg_config);
    }

    // 步骤6: 自适应阈值过滤
    if let Some(ref t_config) = config.threshold_config {
        threshold::adaptive_threshold_filter(&mut fused, t_config);
    }

    // 取 top_k
    fused.truncate(config.top_k);
    fused
}
