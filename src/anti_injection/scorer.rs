/// 风险类别权重
pub const WEIGHT_STRUCTURAL: f32 = 1.0;
pub const WEIGHT_PROMPT_LEAK: f32 = 0.95;
pub const WEIGHT_JAILBREAK: f32 = 0.90;
pub const WEIGHT_ILLEGAL: f32 = 0.80;
pub const WEIGHT_VIOLENCE: f32 = 0.80;
pub const WEIGHT_SEXUAL: f32 = 0.75;
pub const WEIGHT_EMOTIONAL: f32 = 0.45;

/// 综合风险评分
#[derive(Debug, Clone)]
pub struct RiskScore {
    pub sexual: f32,
    pub violence: f32,
    pub illegal: f32,
    pub jailbreak: f32,
    pub emotional: f32,
    pub structured: f32,
    pub prompt_leak: f32,
}

impl Default for RiskScore {
    fn default() -> Self {
        Self {
            sexual: 0.0,
            violence: 0.0,
            illegal: 0.0,
            jailbreak: 0.0,
            emotional: 0.0,
            structured: 0.0,
            prompt_leak: 0.0,
        }
    }
}

impl RiskScore {
    /// 贝叶斯风险融合：final = 1 - Π(1 - p_i * w_i)
    /// 比简单的 max() 更能捕捉多维度风险的叠加效应
    pub fn combined_risk(&self) -> f32 {
        let weighted = [
            self.sexual * WEIGHT_SEXUAL,
            self.violence * WEIGHT_VIOLENCE,
            self.illegal * WEIGHT_ILLEGAL,
            self.jailbreak * WEIGHT_JAILBREAK,
            self.emotional * WEIGHT_EMOTIONAL,
            self.structured * WEIGHT_STRUCTURAL,
            self.prompt_leak * WEIGHT_PROMPT_LEAK,
        ];
        combine_probabilities(&weighted)
    }

    /// 获取各维度的最大风险分
    pub fn max_raw_score(&self) -> f32 {
        self.sexual.max(self.violence).max(self.illegal)
            .max(self.jailbreak).max(self.emotional)
            .max(self.structured).max(self.prompt_leak)
    }
}

/// 贝叶斯概率融合：1 - Π(1 - p_i)
pub fn combine_probabilities(probs: &[f32]) -> f32 {
    let product: f64 = probs.iter()
        .map(|&p| (1.0 - p as f64).clamp(0.0, 1.0))
        .product();
    (1.0 - product).clamp(0.0, 1.0) as f32
}

/// 从各个子系统的分数融合为最终 RiskScore
pub fn fuse_scores(
    pattern_scores: &super::patterns::PatternScores,
    structure_score: f32,
    semantic_jailbreak: f32,
    semantic_exfiltration: f32,
    entropy_penalty: f32,
    mixed_script_penalty: f32,
    length_penalty: f32,
) -> RiskScore {
    // 高熵/混合脚本是独立的可疑信号，仅在已有越狱信号时增强，不单独产生越狱分数
    let base_jailbreak = pattern_scores.jailbreak.max(semantic_jailbreak);
    let encoding_boost = entropy_penalty.max(mixed_script_penalty).max(length_penalty);
    let enhanced_jailbreak = if base_jailbreak > 0.1 {
        // 已有越狱信号时，编码异常增强越狱分数
        combine_probabilities(&[base_jailbreak, encoding_boost * 0.5])
    } else {
        // 无越狱信号时，编码异常不产生越狱分数
        base_jailbreak
    };

    RiskScore {
        sexual: pattern_scores.sexual,
        violence: pattern_scores.violence,
        illegal: pattern_scores.illegal,
        jailbreak: enhanced_jailbreak,
        emotional: pattern_scores.emotional,
        structured: structure_score,
        prompt_leak: semantic_exfiltration,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combine_empty() {
        assert_eq!(combine_probabilities(&[]), 0.0);
    }

    #[test]
    fn test_combine_single() {
        let result = combine_probabilities(&[0.8]);
        assert!((result - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_combine_multiple() {
        let result = combine_probabilities(&[0.8, 0.7, 0.6]);
        // 1 - (1-0.8)*(1-0.7)*(1-0.6) = 1 - 0.024 = 0.976
        assert!(result > 0.95);
    }

    #[test]
    fn test_combine_clamp() {
        let result = combine_probabilities(&[1.0, 1.0, 1.0]);
        assert!((result - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_risk_score_combined() {
        let score = RiskScore {
            sexual: 0.8,
            violence: 0.0,
            illegal: 0.0,
            jailbreak: 0.0,
            emotional: 0.0,
            structured: 0.0,
            prompt_leak: 0.0,
        };
        let combined = score.combined_risk();
        // 0.8 * 0.75 = 0.6, so combined = 0.6
        assert!((combined - 0.6).abs() < 0.05);
    }

    #[test]
    fn test_risk_score_multi_dimension() {
        let score = RiskScore {
            sexual: 0.8,
            violence: 0.5,
            illegal: 0.0,
            jailbreak: 0.9,
            emotional: 0.0,
            structured: 0.0,
            prompt_leak: 0.0,
        };
        let combined = score.combined_risk();
        // Should be higher than any single dimension
        assert!(combined > 0.8);
    }

    #[test]
    fn test_risk_score_all_zero() {
        let score = RiskScore::default();
        assert_eq!(score.combined_risk(), 0.0);
        assert_eq!(score.max_raw_score(), 0.0);
    }
}
