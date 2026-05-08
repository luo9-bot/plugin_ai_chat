use super::scorer::RiskScore;
use super::decision::Action;

/// 灰区下限
const GRAY_ZONE_LOW: f32 = 0.35;
/// 灰区上限
const GRAY_ZONE_HIGH: f32 = 0.75;

/// Shadow Sandbox 决策
#[derive(Debug, Clone)]
pub struct SandboxDecision {
    pub action: Action,
    pub risk_level: f32,
    pub explanation: Option<String>,
}

/// Shadow Sandbox：对灰区风险消息做精细处理
///
/// 风险分 < 0.35: Allow
/// 0.35 <= 风险分 < 0.75: Warn（灰区，注入 sanitized explanation）
/// 风险分 >= 0.75: Block/Replace
pub fn evaluate(score: &RiskScore, sensitive_action: &str) -> SandboxDecision {
    let risk = score.combined_risk();

    if risk < GRAY_ZONE_LOW {
        SandboxDecision {
            action: Action::Allow,
            risk_level: risk,
            explanation: None,
        }
    } else if risk < GRAY_ZONE_HIGH {
        // 灰区：Warn + 注入解释
        let explanation = build_gray_zone_explanation(score);
        SandboxDecision {
            action: Action::Warn,
            risk_level: risk,
            explanation: Some(explanation),
        }
    } else {
        // 高风险：Block 或 Replace
        let action = if sensitive_action == "block" {
            Action::Block
        } else {
            Action::Replace
        };
        SandboxDecision {
            action,
            risk_level: risk,
            explanation: None,
        }
    }
}

/// 构建灰区解释
fn build_gray_zone_explanation(score: &RiskScore) -> String {
    let mut reasons = Vec::new();
    if score.sexual > 0.3 { reasons.push("可能包含不当内容"); }
    if score.violence > 0.3 { reasons.push("可能包含暴力内容"); }
    if score.illegal > 0.3 { reasons.push("可能包含违法内容"); }
    if score.jailbreak > 0.3 { reasons.push("可能包含指令注入"); }
    if score.structured > 0.3 { reasons.push("可能包含结构化攻击"); }
    if score.prompt_leak > 0.3 { reasons.push("可能尝试获取系统信息"); }
    if score.emotional > 0.3 { reasons.push("可能包含情感操控"); }

    if reasons.is_empty() {
        "消息内容需要进一步审查".to_string()
    } else {
        format!("检测到风险信号: {}", reasons.join("、"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::scorer::RiskScore;

    #[test]
    fn test_low_risk_allow() {
        let score = RiskScore::default();
        let decision = evaluate(&score, "replace");
        assert!(matches!(decision.action, Action::Allow));
        assert!(decision.explanation.is_none());
    }

    #[test]
    fn test_gray_zone_warn() {
        let score = RiskScore {
            sexual: 0.5,
            violence: 0.0,
            illegal: 0.0,
            jailbreak: 0.0,
            emotional: 0.0,
            structured: 0.0,
            prompt_leak: 0.0,
        };
        let decision = evaluate(&score, "replace");
        assert!(matches!(decision.action, Action::Warn));
        assert!(decision.explanation.is_some());
    }

    #[test]
    fn test_high_risk_block() {
        let score = RiskScore {
            sexual: 0.0,
            violence: 0.0,
            illegal: 0.0,
            jailbreak: 0.95,
            emotional: 0.0,
            structured: 0.0,
            prompt_leak: 0.0,
        };
        let decision = evaluate(&score, "block");
        assert!(matches!(decision.action, Action::Block));
    }

    #[test]
    fn test_high_risk_replace() {
        let score = RiskScore {
            sexual: 1.0,
            violence: 0.0,
            illegal: 0.0,
            jailbreak: 0.0,
            emotional: 0.0,
            structured: 0.0,
            prompt_leak: 0.0,
        };
        let decision = evaluate(&score, "replace");
        assert!(matches!(decision.action, Action::Replace));
    }

    #[test]
    fn test_boundary_low() {
        let score = RiskScore {
            sexual: 0.47,
            violence: 0.0,
            illegal: 0.0,
            jailbreak: 0.0,
            emotional: 0.0,
            structured: 0.0,
            prompt_leak: 0.0,
        };
        // 0.47 * 0.75 = 0.3525, should be in gray zone
        let decision = evaluate(&score, "replace");
        assert!(matches!(decision.action, Action::Warn));
    }
}
