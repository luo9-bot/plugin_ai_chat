//! 自适应阈值过滤
//!
//! 基于分数分布的动态阈值过滤：
//! threshold = mean(scores) - 0.5 * std(scores)
//!
//! 这条规则成立的**前提**是输入分数有绝对尺度。融合分数现在由
//! `fusion::weighted_rrf_fusion` 归一化到 [0,1]，因此阈值是有意义的；
//! 在那之前融合分数的取值带是约 `[0.0037, 0.0164]` 而 `min_score` 是 `0.0`，
//! 阈值恒被下限截断，这个过滤器实际上没有过滤任何东西。

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
pub fn adaptive_threshold_filter(results: &mut Vec<RetrievalResult>, config: &ThresholdConfig) {
    if results.is_empty() {
        return;
    }

    let threshold = if config.adaptive && results.len() > 1 {
        let scores: Vec<f64> = results.iter().map(|r| r.score).collect();
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;
        let variance = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / scores.len() as f64;
        let std = variance.sqrt();
        let adaptive_threshold = mean - config.std_multiplier * std;
        adaptive_threshold.max(config.min_score)
    } else {
        config.min_score
    };

    results.retain(|r| r.score >= threshold);
}
