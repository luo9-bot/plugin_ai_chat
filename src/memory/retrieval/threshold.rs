//! 自适应阈值过滤
//!
//! 基于分数分布的动态阈值过滤：
//! threshold = mean(scores) - 0.5 * std(scores)

use super::fusion::RetrievalResult;

/// 自适应阈值配置
pub struct ThresholdConfig {
    /// 最小分数阈值（绝对下限）
    pub min_score: f64,
    /// 是否启用自适应
    pub adaptive: bool,
    /// 标准差倍数（阈值 = mean - multiplier * std）
    pub std_multiplier: f64,
}

impl Default for ThresholdConfig {
    fn default() -> Self {
        Self {
            min_score: 0.0,
            adaptive: true,
            std_multiplier: 0.5,
        }
    }
}

/// 自适应阈值过滤
///
/// 计算分数分布的均值和标准差，
/// 过滤掉低于 `mean - multiplier * std` 的结果。
pub fn adaptive_threshold_filter(
    results: &mut Vec<RetrievalResult>,
    config: &ThresholdConfig,
) {
    if results.is_empty() {
        return;
    }

    let threshold = if config.adaptive && results.len() > 1 {
        let scores: Vec<f64> = results.iter().map(|r| r.score).collect();
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;
        let variance = scores
            .iter()
            .map(|s| (s - mean).powi(2))
            .sum::<f64>()
            / scores.len() as f64;
        let std = variance.sqrt();
        let adaptive_threshold = mean - config.std_multiplier * std;
        adaptive_threshold.max(config.min_score)
    } else {
        config.min_score
    };

    results.retain(|r| r.score >= threshold);
}

/// Min-Max 归一化
pub fn normalize_scores_minmax(results: &mut [RetrievalResult]) {
    if results.is_empty() {
        return;
    }
    let min = results
        .iter()
        .map(|r| r.score)
        .fold(f64::INFINITY, f64::min);
    let max = results
        .iter()
        .map(|r| r.score)
        .fold(f64::NEG_INFINITY, f64::max);

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
