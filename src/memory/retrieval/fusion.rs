//! Weighted RRF (Reciprocal Rank Fusion) 融合
//!
//! 融合向量检索和 BM25 检索的结果
//! score = w_vec / (k + rank_vec) + w_bm25 / (k + rank_bm25)

use std::collections::HashMap;

use super::bm25::Bm25Result;
use super::vector::VectorResult;

/// 融合后的检索结果
#[derive(Debug, Clone)]
pub struct RetrievalResult {
    pub id: String,
    pub content: String,
    pub score: f64,
    pub source: &'static str,
}

/// Weighted RRF 融合
pub fn weighted_rrf_fusion(
    vector_results: &[VectorResult],
    bm25_results: &[Bm25Result],
    rrf_k: f64,
    vector_weight: f64,
    bm25_weight: f64,
) -> Vec<RetrievalResult> {
    let mut scores: HashMap<String, f64> = HashMap::new();

    // 向量结果贡献分数（1-based rank）
    for (rank, result) in vector_results.iter().enumerate() {
        let rrf_score = vector_weight / (rrf_k + (rank + 1) as f64);
        *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
    }

    // BM25 结果贡献分数（1-based rank）
    for (rank, result) in bm25_results.iter().enumerate() {
        let rrf_score = bm25_weight / (rrf_k + (rank + 1) as f64);
        *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
    }

    // 构建结果列表
    let mut results: Vec<RetrievalResult> = scores
        .into_iter()
        .map(|(id, score)| {
            let source = if vector_results.iter().any(|r| r.id == id)
                && bm25_results.iter().any(|r| r.id == id)
            {
                "fusion_rrf"
            } else if vector_results.iter().any(|r| r.id == id) {
                "vector"
            } else {
                "bm25"
            };
            RetrievalResult {
                id,
                content: String::new(), // 填充由调用方完成
                score,
                source,
            }
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results
}

/// Min-Max 归一化
#[allow(dead_code)]
pub fn normalize_scores_minmax(results: &mut [RetrievalResult]) {
    if results.is_empty() {
        return;
    }
    let min = results.iter().map(|r| r.score).fold(f64::INFINITY, f64::min);
    let max = results.iter().map(|r| r.score).fold(f64::NEG_INFINITY, f64::max);

    if (max - min).abs() < 1e-12 {
        for r in results.iter_mut() {
            r.score = 1.0;
        }
    } else {
        for r in results.iter_mut() {
            r.score = (r.score - min) / (max - min);
        }
    }
}
