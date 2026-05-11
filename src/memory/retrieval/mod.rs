//! 记忆检索模块
//!
//! 双路检索：向量语义搜索 + BM25 关键词搜索
//! 使用 Weighted RRF 融合两路结果

mod bm25;
mod vector;
mod fusion;

pub use fusion::RetrievalResult;

use tracing::{debug, info};

/// 双路检索配置
pub struct RetrievalConfig {
    pub top_k: usize,
    pub vector_weight: f64,
    pub bm25_weight: f64,
    pub rrf_k: f64,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: 10,
            vector_weight: 0.7,
            bm25_weight: 0.3,
            rrf_k: 60.0,
        }
    }
}

/// 双路检索：向量 + BM25 + RRF 融合
pub fn dual_path_retrieve(
    query: &str,
    memories: &[(String, String)], // (id, content)
    embeddings: &[(String, Vec<f32>)], // (id, embedding)
    config: &RetrievalConfig,
) -> Vec<RetrievalResult> {
    // BM25 关键词检索
    let bm25_results = bm25::search(query, memories, config.top_k);

    // 向量语义检索
    let vector_results = if let Some(query_embedding) = vector::embed_query(query) {
        vector::search(&query_embedding, embeddings, config.top_k)
    } else {
        Vec::new()
    };

    // Weighted RRF 融合
    let fused = fusion::weighted_rrf_fusion(
        &vector_results,
        &bm25_results,
        config.rrf_k,
        config.vector_weight,
        config.bm25_weight,
    );

    // 取 top_k
    let mut results = fused;
    results.truncate(config.top_k);
    results
}
