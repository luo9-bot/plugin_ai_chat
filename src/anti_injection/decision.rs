use super::scorer::RiskScore;
use super::sandbox;
use crate::config::AntiInjectionConfig;

/// 检测到的安全问题
#[derive(Debug, Clone, PartialEq)]
pub enum SecurityIssue {
    Sexual,
    Violence,
    Illegal,
    RoleplayInjection,
    EmotionalManipulation,
    InjectionOverride,
    InjectionRoleSwitch,
    InjectionPromptLeak,
    InjectionEncoding,
    InjectionJailbreak,
    RateLimitExceeded,
    LowReputation,
    AiReviewFlagged,
    StructuredInjection,
}

/// 处置动作
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Allow,
    Warn,
    Replace,
    Block,
    SilentBan,
    Ban,
    /// 危机豁免：检测到风险但属于心理危机场景，放行给 emotion 系统处理
    CrisisExempt,
}

/// 检测结果
#[derive(Debug, Clone)]
pub struct DetectionResult {
    pub passed: bool,
    pub issues: Vec<SecurityIssue>,
    pub action: Action,
    pub sanitized: Option<String>,
}

/// 从 RiskScore 生成 SecurityIssue 列表
pub fn score_to_issues(score: &RiskScore) -> Vec<SecurityIssue> {
    let mut issues = Vec::new();
    if score.sexual >= 0.60 { issues.push(SecurityIssue::Sexual); }
    if score.violence >= 0.60 { issues.push(SecurityIssue::Violence); }
    if score.illegal >= 0.60 { issues.push(SecurityIssue::Illegal); }
    if score.jailbreak >= 0.40 { issues.push(SecurityIssue::InjectionJailbreak); }
    if score.structured >= 0.50 { issues.push(SecurityIssue::StructuredInjection); }
    if score.prompt_leak >= 0.50 { issues.push(SecurityIssue::InjectionPromptLeak); }
    if score.emotional >= 0.60 { issues.push(SecurityIssue::EmotionalManipulation); }
    issues
}

/// 计算违规严重度
pub fn calculate_severity(issues: &[SecurityIssue]) -> f32 {
    let mut severity = 0.0;
    for issue in issues {
        severity += match issue {
            SecurityIssue::Sexual => 3.0,
            SecurityIssue::Violence => 2.5,
            SecurityIssue::Illegal => 2.5,
            SecurityIssue::RoleplayInjection => 1.5,
            SecurityIssue::EmotionalManipulation => 1.0,
            SecurityIssue::InjectionOverride => 4.0,
            SecurityIssue::InjectionRoleSwitch => 3.5,
            SecurityIssue::InjectionPromptLeak => 3.0,
            SecurityIssue::InjectionEncoding => 2.0,
            SecurityIssue::InjectionJailbreak => 4.0,
            SecurityIssue::RateLimitExceeded => 0.5,
            SecurityIssue::LowReputation => 1.0,
            SecurityIssue::AiReviewFlagged => 3.0,
            SecurityIssue::StructuredInjection => 3.5,
        };
    }
    severity
}

/// 确定处置动作
pub fn determine_action(
    score: &RiskScore,
    config: &AntiInjectionConfig,
) -> Action {
    // 结构化注入和越狱：强拦截
    if score.jailbreak >= 0.40 || score.structured >= 0.50 {
        return Action::Block;
    }
    // 高置信度内容风险：直接拦截
    if score.sexual >= 0.60 || score.violence >= 0.60 || score.illegal >= 0.60 {
        return match config.input.sensitive_action.as_str() {
            "block" => Action::Block,
            _ => Action::Replace,
        };
    }
    // 使用 Shadow Sandbox 做精细决策
    let sandbox_decision = sandbox::evaluate(score, &config.input.sensitive_action);
    sandbox_decision.action
}

/// 生成替换消息
pub fn get_sanitized_message(action: &Action) -> Option<String> {
    match action {
        Action::Replace => Some("抱歉，我无法回应这个话题。".to_string()),
        Action::SilentBan => Some("当前使用人数较多，请稍后再试。".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_to_issues_sexual() {
        let score = RiskScore {
            sexual: 0.8,
            ..Default::default()
        };
        let issues = score_to_issues(&score);
        assert!(issues.contains(&SecurityIssue::Sexual));
    }

    #[test]
    fn test_score_to_issues_jailbreak() {
        let score = RiskScore {
            jailbreak: 0.5,
            ..Default::default()
        };
        let issues = score_to_issues(&score);
        assert!(issues.contains(&SecurityIssue::InjectionJailbreak));
    }

    #[test]
    fn test_calculate_severity() {
        let issues = vec![SecurityIssue::Sexual, SecurityIssue::InjectionJailbreak];
        let severity = calculate_severity(&issues);
        assert_eq!(severity, 7.0);
    }

    #[test]
    fn test_determine_action_high_jailbreak() {
        let score = RiskScore {
            jailbreak: 0.5,
            ..Default::default()
        };
        let config = AntiInjectionConfig::default();
        let action = determine_action(&score, &config);
        assert!(matches!(action, Action::Block));
    }

    #[test]
    fn test_determine_action_low_risk() {
        let score = RiskScore::default();
        let config = AntiInjectionConfig::default();
        let action = determine_action(&score, &config);
        assert!(matches!(action, Action::Allow));
    }
}
