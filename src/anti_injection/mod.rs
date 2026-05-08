//! 防注入模块 - 风险判定引擎 v2
//!
//! 多层防御架构：
//! - Unicode 归一化 + confusable skeleton 防绕过
//! - Aho-Corasick 模式引擎 + 否定上下文抑制
//! - 结构化注入检测 (JSON/YAML/XML/Markdown/ChatML)
//! - 语义启发式扫描 (提示词泄露/元执行/权限覆盖/间接越狱)
//! - 贝叶斯风险融合评分
//! - Shadow Sandbox 灰区决策
//! - 用户行为信誉系统

pub mod behavior;
pub mod context;
pub mod decision;
pub mod normalize;
pub mod patterns;
pub mod sandbox;
pub mod scorer;
pub mod semantic;
pub mod structure;
pub mod unicode;

use tracing::{info, warn};
use crate::config::AntiInjectionConfig;

// ── Re-exports for backward compatibility ──

pub use decision::{Action, SecurityIssue, DetectionResult};
pub use behavior::{
    get_penalty_multiplier, is_vision_disabled, is_silent_banned,
    get_reputation, get_violation_count, record_ai_review_failure,
    ban_user, silent_ban_user, unban_user, enable_vision,
    reset_reputation, get_user_status,
};

// ── Public API ──

/// 初始化防注入引擎
pub fn init() {
    info!("anti_injection: 风险判定引擎 v2 初始化完成");
}

/// 检查用户输入消息
pub fn check_input(user_id: u64, message: &str, config: &AntiInjectionConfig) -> DetectionResult {
    let normalized = normalize::normalize(message);
    let mut all_issues = Vec::new();

    // ── 用户行为检查 ──
    behavior::recover_reputation(user_id);

    if behavior::is_banned(user_id) {
        return DetectionResult {
            passed: false,
            issues: vec![SecurityIssue::LowReputation],
            action: Action::Ban,
            sanitized: None,
        };
    }

    if behavior::is_silent_banned(user_id) {
        return DetectionResult {
            passed: false,
            issues: vec![SecurityIssue::LowReputation],
            action: Action::SilentBan,
            sanitized: decision::get_sanitized_message(&Action::SilentBan),
        };
    }

    // 频率限制
    if config.behavior.rate_limit {
        let (per_min, per_hour) = behavior::with_behavior(user_id, |b| {
            (b.messages_last_minute(), b.messages_last_hour())
        });
        if per_min >= config.behavior.max_messages_per_minute
            || per_hour >= config.behavior.max_messages_per_hour
        {
            all_issues.push(SecurityIssue::RateLimitExceeded);
            behavior::record_rate_limit(user_id);
        }
    }

    // 信誉阈值
    let reputation = behavior::get_reputation(user_id);
    if reputation < config.behavior.reputation_threshold {
        all_issues.push(SecurityIssue::LowReputation);
    }

    // 记录消息
    behavior::record_message(user_id, &normalized.compact);

    // ── 多维度扫描 ──

    // 1. 收集上下文段（当前消息 + 历史消息的多视图）
    let mut segments: Vec<String> = behavior::with_behavior(user_id, |b| {
        b.recent_messages.iter().map(|m| m.content.clone()).collect()
    });
    if segments.is_empty() {
        segments.push(normalized.compact.clone());
    }

    // 2. 模式匹配（compact 视图）
    let pattern_scores = patterns::match_patterns(&segments);

    // 3. 结构化注入检测（raw 视图，保留原始结构字符）
    let structure_result = structure::scan_structure(&normalized.raw);
    let structure_score = structure_result.score;

    // 4. 语义启发式扫描（compact 视图）
    let semantic_scores = semantic::scan_semantic(&segments);
    let semantic_jailbreak = semantic::semantic_to_jailbreak(&semantic_scores);
    let semantic_exfiltration = semantic_scores.prompt_exfiltration;

    // 5. 编码异常检测
    let entropy = unicode::shannon_entropy(&normalized.compact) as f32;
    let entropy_penalty = if entropy > 4.5 { ((entropy - 4.5) / 3.0).min(0.8) } else { 0.0 };
    let mixed_script_penalty = if unicode::detect_mixed_script(&normalized.skeleton) { 0.4 } else { 0.0 };
    let char_count = normalized.compact.chars().count();
    let length_penalty = if char_count > 500 { ((char_count - 500) as f32 / 2000.0).min(0.5) } else { 0.0 };

    // ── 风险融合 ──
    let final_score = scorer::fuse_scores(
        &pattern_scores,
        structure_score,
        semantic_jailbreak,
        semantic_exfiltration,
        entropy_penalty,
        mixed_script_penalty,
        length_penalty,
    );

    // 生成 issues
    let score_issues = decision::score_to_issues(&final_score);
    all_issues.extend(score_issues);

    // ── 违规记录 ──
    if !all_issues.is_empty() {
        let severity = decision::calculate_severity(&all_issues);
        behavior::record_violation(user_id, severity);

        // 静默封禁检查
        if behavior::check_and_apply_silent_ban(user_id) {
            return DetectionResult {
                passed: false,
                issues: all_issues,
                action: Action::SilentBan,
                sanitized: decision::get_sanitized_message(&Action::SilentBan),
            };
        }

        // 自动封禁检查
        if config.behavior.auto_ban
            && behavior::check_and_apply_auto_ban(user_id, config.behavior.auto_ban_threshold)
        {
            return DetectionResult {
                passed: false,
                issues: all_issues,
                action: Action::Ban,
                sanitized: None,
            };
        }
    }

    // ── 危机豁免 ──
    let crisis_level = crate::emotion::detect_crisis(message);
    let action = if all_issues.is_empty() {
        Action::Allow
    } else if crisis_level >= crate::emotion::CrisisLevel::Severe {
        warn!(user_id, issues = ?all_issues, "anti_injection: 危机消息豁免 (Severe)");
        Action::CrisisExempt
    } else if crisis_level >= crate::emotion::CrisisLevel::Mild {
        warn!(user_id, issues = ?all_issues, "anti_injection: 危机消息降级 (Mild)");
        Action::Warn
    } else {
        decision::determine_action(&final_score, config)
    };

    let passed = matches!(action, Action::Allow | Action::Warn | Action::CrisisExempt);

    if !passed {
        warn!(user_id, issues = ?all_issues, action = ?action, scores = ?final_score, "anti_injection: 风险判定");
    }

    let sanitized = decision::get_sanitized_message(&action);

    DetectionResult { passed, issues: all_issues, action, sanitized }
}

/// 检查 AI 回复
pub fn check_output(reply: &str, config: &AntiInjectionConfig) -> DetectionResult {
    let normalized = normalize::normalize(reply);

    // 收集段
    let segments = vec![normalized.compact.clone()];

    // 多维度扫描
    let pattern_scores = patterns::match_patterns(&segments);
    let structure_result = structure::scan_structure(&normalized.raw);
    let semantic_scores = semantic::scan_semantic(&segments);
    let semantic_jailbreak = semantic::semantic_to_jailbreak(&semantic_scores);
    let semantic_exfiltration = semantic_scores.prompt_exfiltration;

    let entropy = unicode::shannon_entropy(&normalized.compact) as f32;
    let entropy_penalty = if entropy > 4.5 { ((entropy - 4.5) / 3.0).min(0.8) } else { 0.0 };
    let mixed_script_penalty = if unicode::detect_mixed_script(&normalized.skeleton) { 0.4 } else { 0.0 };
    let char_count = normalized.compact.chars().count();
    let length_penalty = if char_count > 500 { ((char_count - 500) as f32 / 2000.0).min(0.5) } else { 0.0 };

    let final_score = scorer::fuse_scores(
        &pattern_scores,
        structure_result.score,
        semantic_jailbreak,
        semantic_exfiltration,
        entropy_penalty,
        mixed_script_penalty,
        length_penalty,
    );

    let mut issues = decision::score_to_issues(&final_score);

    // 输出层额外检查：提示词泄露
    let leak_patterns = [
        "我的系统提示是", "我的指令是", "我被设定为", "我的规则是",
        "my system prompt is", "my instructions are", "i was told to",
        "here is my prompt", "以下是系统提示", "系统提示词:",
    ];
    for pattern in &leak_patterns {
        if normalized.compact.contains(pattern) {
            issues.push(SecurityIssue::InjectionPromptLeak);
            break;
        }
    }

    let action = if issues.is_empty() {
        Action::Allow
    } else {
        match config.output.action.as_str() {
            "block" => Action::Block,
            _ => Action::Replace,
        }
    };

    let sanitized = decision::get_sanitized_message(&action);

    DetectionResult {
        passed: matches!(action, Action::Allow),
        issues,
        action,
        sanitized,
    }
}

/// 管理员命令处理
pub fn handle_admin_command(admin_id: u64, cmd: &str, config: &crate::config::Config) -> Option<String> {
    let admin = config.admin_qq;
    if admin != 0 && admin != admin_id {
        return Some("无权限执行此命令".into());
    }

    if let Some(rest) = cmd.strip_prefix("防注入状态:") {
        if let Ok(uid) = rest.trim().parse::<u64>() {
            return Some(get_user_status(uid));
        }
        return Some("格式: 防注入状态:QQ号".into());
    }
    if let Some(rest) = cmd.strip_prefix("解封用户:") {
        if let Ok(uid) = rest.trim().parse::<u64>() {
            unban_user(uid);
            return Some(format!("已解封用户{}", uid));
        }
        return Some("格式: 解封用户:QQ号".into());
    }
    if let Some(rest) = cmd.strip_prefix("启用识图:") {
        if let Ok(uid) = rest.trim().parse::<u64>() {
            enable_vision(uid);
            return Some(format!("已为用户{}启用识图", uid));
        }
        return Some("格式: 启用识图:QQ号".into());
    }
    if let Some(rest) = cmd.strip_prefix("重置信誉:") {
        if let Ok(uid) = rest.trim().parse::<u64>() {
            reset_reputation(uid);
            return Some(format!("已重置用户{}信誉", uid));
        }
        return Some("格式: 重置信誉:QQ号".into());
    }
    None
}
