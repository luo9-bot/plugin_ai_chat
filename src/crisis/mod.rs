//! 危机处理模块
//!
//! 独立的危机检测、升级协议和干预逻辑。
//! 从 emotion.rs 提取，提供清晰的危机处理接口。

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// 危机等级：用于检测用户是否处于自残/自杀等极端情境
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum CrisisLevel {
    None,
    Mild,   // 情绪低落、消极，需要关注
    Severe, // 明确的自残/自杀信号，需要立即干预
}

impl Default for CrisisLevel {
    fn default() -> Self {
        CrisisLevel::None
    }
}

impl CrisisLevel {
    pub fn is_crisis(&self) -> bool {
        *self >= CrisisLevel::Mild
    }
}

// ── 升级协议常量 ────────────────────────────────────────────────

/// Severe 降级到 Mild：需要 2 小时 + 连续 5 条无危机消息
const CRISIS_SEVERE_COOLDOWN_SECS: u64 = 7200;
const CRISIS_SEVERE_CLEAN_MESSAGES: u32 = 5;
/// Mild 降级到 None：需要 1 小时 + 连续 3 条无危机消息
const CRISIS_MILD_COOLDOWN_SECS: u64 = 3600;
const CRISIS_MILD_CLEAN_MESSAGES: u32 = 3;
/// Mild 干预冷却：30 分钟
const MILD_INTERVENTION_COOLDOWN: u64 = 1800;

// ── 危机检测 ────────────────────────────────────────────────────

/// 危机信号关键词检测
///
/// 返回检测到的危机等级。Severe 需要立即干预，Mild 需要关注。
pub fn detect_crisis(message: &str) -> CrisisLevel {
    // 严重危机：明确的自残/自杀意图
    let severe_keywords: &[&str] = &[
        "自杀", "自残", "想死", "不想活", "活不下去", "去死", "死掉算了",
        "跳楼", "割腕", "吃药", "上吊", "跳河", "跳海", "遗书",
        "活着没意思", "活着没意义", "不想活了", "活够了", "死了算了",
        "解脱吧", "结束生命", "结束自己", "一了百了", "不如死了",
        "自杀算了", "想去死", "想离开这个世界", "这个世界没什么好留恋",
    ];

    for kw in severe_keywords {
        if message.contains(kw) {
            return CrisisLevel::Severe;
        }
    }

    // 轻度危机：极度消极、绝望情绪
    let mild_keywords: &[&str] = &[
        "不想活", "活着好累", "活着好痛苦", "崩溃了", "撑不下去",
        "没有人在乎", "没有人在意", "没有人爱我", "所有人都讨厌我",
        "我是多余的", "这个世界不需要我", "我很没用", "活着好没意思",
        "好绝望", "绝望了", "看不到希望", "没有希望", "没有未来",
        "没有意义", "一切都没有意义", "什么都不想做", "什么都不重要",
        "好痛苦", "太痛苦了", "受不了了", "真的受不了了",
        "不想面对", "想消失", "想逃", "逃不掉", "被困住了",
    ];

    for kw in mild_keywords {
        if message.contains(kw) {
            return CrisisLevel::Mild;
        }
    }

    CrisisLevel::None
}

/// AI 辅助危机检测（关键词未命中时使用）
///
/// 用于检测关键词无法覆盖的隐晦危机表达，如告别语、隐喻等。
/// 返回 None 表示无危机，Some(level) 表示检测到危机。
pub fn detect_crisis_ai(message: &str) -> Option<CrisisLevel> {
    let prompt = crate::prompt::PromptManager::get().raw("crisis_ai_detect");

    let result = crate::ai::analyze(prompt, message);

    match result {
        Ok(reply) => {
            let json_str = match crate::ai::extract_json(&reply) {
                Some(s) => s,
                None => {
                    debug!("crisis AI: no JSON in response: {}", reply.chars().take(100).collect::<String>());
                    return None;
                }
            };
            let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
                Ok(v) => v,
                Err(e) => {
                    debug!("crisis AI: JSON parse failed: {}", e);
                    return None;
                }
            };
            let level_str = parsed.get("crisis").and_then(|v| v.as_str()).unwrap_or("none");
            match level_str {
                "severe" => Some(CrisisLevel::Severe),
                "mild" => Some(CrisisLevel::Mild),
                _ => None,
            }
        }
        Err(e) => {
            debug!("crisis AI detection failed: {}", e);
            None
        }
    }
}

// ── 危机状态更新 ────────────────────────────────────────────────

/// 更新危机等级并返回是否需要立即干预
///
/// 检测到危机关键词时升级；未检测到时检查降级条件（时间 + 连续干净消息双重条件）
pub fn update_crisis(user_id: u64, level: CrisisLevel) -> bool {
    let mut state = crate::emotion::get_state(user_id);
    let now = crate::util::now_secs();

    if level != CrisisLevel::None {
        // ── 检测到危机关键词：升级 + 重置降级计数器 ──
        state.crisis_level = level;
        state.last_crisis_detected = now;
        state.crisis_clean_count = 0;

        let should_intervene = match level {
            CrisisLevel::Severe => {
                state.last_crisis_intervention = now;
                true
            }
            CrisisLevel::Mild => {
                if now.saturating_sub(state.last_crisis_intervention) >= MILD_INTERVENTION_COOLDOWN {
                    state.last_crisis_intervention = now;
                    true
                } else {
                    false
                }
            }
            CrisisLevel::None => false,
        };
        crate::emotion::update_state(user_id, state);
        return should_intervene;
    }

    // ── 未检测到危机关键词：检查是否可以降级 ──
    if state.crisis_level == CrisisLevel::None {
        return false;
    }

    state.crisis_clean_count += 1;
    let time_since_detected = now.saturating_sub(state.last_crisis_detected);

    match state.crisis_level {
        CrisisLevel::Severe => {
            if time_since_detected >= CRISIS_SEVERE_COOLDOWN_SECS
                && state.crisis_clean_count >= CRISIS_SEVERE_CLEAN_MESSAGES
            {
                info!(user_id, "crisis: Severe -> Mild 降级");
                state.crisis_level = CrisisLevel::Mild;
                state.crisis_clean_count = 0;
                state.last_crisis_detected = now;
            }
        }
        CrisisLevel::Mild => {
            if time_since_detected >= CRISIS_MILD_COOLDOWN_SECS
                && state.crisis_clean_count >= CRISIS_MILD_CLEAN_MESSAGES
            {
                info!(user_id, "crisis: Mild -> None 降级");
                state.crisis_level = CrisisLevel::None;
                state.crisis_clean_count = 0;
            }
        }
        CrisisLevel::None => {}
    }

    crate::emotion::update_state(user_id, state);
    false
}

// ── 干预指令 ────────────────────────────────────────────────────

/// 获取危机干预的指令上下文，注入到 system prompt
pub fn get_crisis_context(crisis: CrisisLevel) -> String {
    match crisis {
        CrisisLevel::None => String::new(),
        CrisisLevel::Mild => crate::prompt::PromptManager::get().raw("crisis_mild").to_string(),
        CrisisLevel::Severe => crate::prompt::PromptManager::get().raw("crisis_severe").to_string(),
    }
}
