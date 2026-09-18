use super::decision::Action;
use super::scorer::RiskScore;

/// 灰区下限
const GRAY_ZONE_LOW: f32 = 0.35;
/// 灰区上限
const GRAY_ZONE_HIGH: f32 = 0.75;

/// Shadow Sandbox：对灰区风险消息做精细处理
///
/// 风险分 < 0.35: Allow
/// 0.35 <= 风险分 < 0.75: Warn（灰区）
/// 风险分 >= 0.75: Block/Replace
///
/// 这里只返回处置动作。它曾经还带一个 `risk_level` 和一段"灰区解释"（"检测到
/// 风险信号：可能包含指令注入"），但唯一的调用方
/// （[`super::decision::determine_action`]）只取动作——那段解释既没进 prompt
/// 也没进日志，只有它自己的单元测试读过。产出没人读的东西不是预留，是把
/// "这里做过判断"伪装成事实，所以删掉；真要让运维看见，就在调用点重新长出来。
pub fn evaluate(score: &RiskScore, sensitive_action: &str) -> Action {
    let risk = score.combined_risk();

    if risk < GRAY_ZONE_LOW {
        Action::Allow
    } else if risk < GRAY_ZONE_HIGH {
        Action::Warn
    } else if sensitive_action == "block" {
        Action::Block
    } else {
        Action::Replace
    }
}

#[cfg(test)]
mod tests {
    use super::super::scorer::RiskScore;
    use super::*;

    #[test]
    fn test_low_risk_allow() {
        let score = RiskScore::default();
        assert!(matches!(evaluate(&score, "replace"), Action::Allow));
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
        assert!(matches!(evaluate(&score, "replace"), Action::Warn));
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
        assert!(matches!(evaluate(&score, "block"), Action::Block));
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
        assert!(matches!(evaluate(&score, "replace"), Action::Replace));
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
        assert!(matches!(evaluate(&score, "replace"), Action::Warn));
    }
}
