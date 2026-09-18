//! Weighted RRF (Reciprocal Rank Fusion) 融合
//!
//! 融合向量检索和 BM25 检索的结果
//! score = w_vec / (k + rank_vec) + w_bm25 / (k + rank_bm25)
//!
//! **分数被归一化到 [0,1]**，这一点是下游一切加权的前提：
//! 原始 RRF 分数的取值带是 `[(w_b)/ (k+top_k), (w_v+w_b)/(k+1)]`。
//! 默认值（`w_v=0.7`、`w_b=0.3`、`k=60`、`top_k=10`）下它是
//! 约 `[0.0037, 0.0164]`——一个比任何下游附加项都小两个数量级的区间。
//! 曾经因此出现两个失效：认知偏差的 `+0.06` 无条件加分把**所有**结果
//! 都顶到 `clamp(0,1)` 的上界（于是排序完全由稳定排序的副作用决定），
//! 自适应阈值 `mean - 0.5·std` 在该区间上恒小于下限 `0.0`（于是从不过滤）。
//!
//! 归一化用的是**固定参考值**（理论满分），不是候选集上的 min-max：
//! min-max 会让同一条记忆在不同候选集里得到不同分数，跨查询不可比，
//! 也就无法设阈值、无法做 A/B。

use std::collections::HashMap;

use super::bm25::Bm25Result;
use super::vector::VectorResult;

/// 融合后的检索结果
#[derive(Debug, Clone)]
pub(crate) struct RetrievalResult {
    pub id: String,
    pub content: String,
    /// 归一化到 [0,1] 的相关性：1.0 = 两条路都排第一
    pub score: f64,
}

/// RRF 的理论满分：两条路都排第一时的加权融合值
fn rrf_ceiling(rrf_k: f64, vector_weight: f64, bm25_weight: f64) -> f64 {
    (vector_weight + bm25_weight) / (rrf_k + 1.0)
}

/// Weighted RRF 融合，分数归一化到 [0,1]
pub(crate) fn weighted_rrf_fusion(
    vector_results: &[VectorResult],
    bm25_results: &[Bm25Result],
    rrf_k: f64,
    vector_weight: f64,
    bm25_weight: f64,
) -> Vec<RetrievalResult> {
    let mut scores: HashMap<String, f64> = HashMap::new();

    // 权重为 0 的一路**不贡献候选**。
    //
    // 这一点不是形式主义：原先即使权重是 0 也会 `entry(...).or_insert(0.0)`，
    // 于是"这一路没有任何结果"与"所有候选同分"在结果里无法区分——
    // 一个只有向量权重的配置在查询向量缺失时会返回一批**零分候选**，
    // 顺序由 HashMap 迭代序决定（实测：召回率看起来正常，其实是碰巧）。
    if vector_weight > 0.0 {
        for (rank, result) in vector_results.iter().enumerate() {
            let rrf_score = vector_weight / (rrf_k + (rank + 1) as f64);
            *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
        }
    }

    if bm25_weight > 0.0 {
        for (rank, result) in bm25_results.iter().enumerate() {
            let rrf_score = bm25_weight / (rrf_k + (rank + 1) as f64);
            *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
        }
    }

    let ceiling = rrf_ceiling(rrf_k, vector_weight, bm25_weight);

    // 构建结果列表
    let mut results: Vec<RetrievalResult> = scores
        .into_iter()
        .map(|(id, raw_score)| {
            RetrievalResult {
                id,
                content: String::new(), // 填充由调用方完成
                // 权重全为 0 时没有可用的参考尺度，保持原始分而不是除以 0
                score: if ceiling > 0.0 {
                    (raw_score / ceiling).clamp(0.0, 1.0)
                } else {
                    raw_score
                },
            }
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(id: &str, rank: usize) -> VectorResult {
        VectorResult {
            id: id.to_string(),
            score: 1.0 - rank as f64 * 0.1,
        }
    }

    fn bm25(id: &str, rank: usize) -> Bm25Result {
        Bm25Result {
            id: id.to_string(),
            score: 10.0 - rank as f64,
            _matched_tokens: 1,
        }
    }

    #[test]
    fn top_of_both_routes_is_exactly_one() {
        let fused = weighted_rrf_fusion(&[vector("a", 0)], &[bm25("a", 0)], 60.0, 0.7, 0.3);
        assert_eq!(fused.len(), 1);
        assert!(
            (fused[0].score - 1.0).abs() < 1e-12,
            "两条路都排第一应恰好得 1.0，实际 {}",
            fused[0].score
        );
    }

    #[test]
    fn single_route_score_reflects_its_weight() {
        // 只有 BM25 命中且排第一 ⇒ 分数就是 bm25 的权重占比
        let fused = weighted_rrf_fusion(&[], &[bm25("b", 0)], 60.0, 0.7, 0.3);
        assert!(
            (fused[0].score - 0.3).abs() < 1e-12,
            "只有 BM25 时分数应为 0.3，实际 {}",
            fused[0].score
        );
    }

    #[test]
    fn scores_stay_inside_unit_range_and_keep_order() {
        let vectors: Vec<VectorResult> = (0..10).map(|i| vector(&format!("v{i}"), i)).collect();
        let bm25s: Vec<Bm25Result> = (0..10).map(|i| bm25(&format!("v{i}"), i)).collect();
        let fused = weighted_rrf_fusion(&vectors, &bm25s, 60.0, 0.7, 0.3);

        assert!(fused.iter().all(|r| (0.0..=1.0).contains(&r.score)));
        // 两条路里都是第一的 v0 必须排在最前
        assert_eq!(fused[0].id, "v0");
        assert!(fused[0].score > fused[1].score);
    }

    /// 权重为 0 的一路不贡献候选
    ///
    /// 这不是"顺手清理"：原先把零权重那一路的候选也插进去（分数 0），
    /// 于是"这一路没有结果"与"所有候选同分"无法区分，排序退化成
    /// HashMap 迭代序——**看起来正常，其实是碰巧**。
    #[test]
    fn a_zero_weight_route_contributes_no_candidates() {
        // 两路都是 0：没有排序信号，因此没有候选
        let none = weighted_rrf_fusion(&[], &[bm25("only", 0)], 60.0, 0.0, 0.0);
        assert!(none.is_empty(), "权重全为 0 时不该返回任何候选");

        // 词法权重为 0：词法结果不出现（也不会除以 0）
        let vector_only = weighted_rrf_fusion(&[], &[bm25("lexical", 0)], 60.0, 0.7, 0.0);
        assert!(vector_only.is_empty(), "BM25 权重为 0 时不该出现词法候选");

        // 反过来：只有词法有权重，它就正常工作
        let lexical_only = weighted_rrf_fusion(&[], &[bm25("lexical", 0)], 60.0, 0.0, 0.7);
        assert_eq!(lexical_only.len(), 1);
        assert!(lexical_only[0].score > 0.0);
    }
}
