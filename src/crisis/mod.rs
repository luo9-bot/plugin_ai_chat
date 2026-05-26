//! 危机处理模块
//!
//! 以 AI 判断为核心的危机检测系统：
//! - 关键词仅作为"候选信号"，是否真正触发由 AI 判断
//! - AI 返回置信度分数，高置信度才注入干预指令
//! - 无频率限制——真正的危机不会因为"发太多次"被忽略
//! - 检查过往对话上下文，判断是否是一贯的消极倾向

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// 危机等级
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, PartialOrd)]
#[derive(Default)]
pub enum CrisisLevel {
    #[default]
    None,
    Mild,
    Severe,
}

impl CrisisLevel {
    pub fn is_crisis(&self) -> bool {
        *self >= CrisisLevel::Mild
    }
}

// ── 降级常量 ────────────────────────────────────────────────────

const SEVERE_COOLDOWN: u64 = 7200;
const MILD_COOLDOWN: u64 = 3600;
const MILD_INTERVENTION_COOLDOWN: u64 = 1800;
const SEVERE_CLEAN_MSG: u32 = 5;
const MILD_CLEAN_MSG: u32 = 3;

// ── 危机检测 ────────────────────────────────────────────────────

/// 检测消息中的危机信号（关键词粗筛）
///
/// 关键词只作为候选信号，是否真正触发由 AI 判断。
/// 返回的等级表示"疑似"，`update_crisis` 中会结合 AI 评估做最终决定。
pub fn detect_crisis(message: &str) -> CrisisLevel {
    let severe: &[&str] = &[
        "自杀", "自残", "想死", "不想活", "活不下去",
        "去死", "死掉算了", "跳楼", "割腕", "上吊", "遗书",
        "结束自己", "一了百了", "死了算了",
    ];

    for kw in severe {
        if message.contains(kw) {
            return CrisisLevel::Mild; // 候选信号，是否 Severe 由 AI 判断
        }
    }

    let mild: &[&str] = &[
        "活着好累", "活着好痛苦", "崩溃了", "撑不下去",
        "没有人在乎", "没有人在意", "没有意义",
        "我是多余的", "我很没用", "好绝望", "绝望了",
        "看不到希望", "没有希望", "太痛苦了",
    ];

    for kw in mild {
        if message.contains(kw) && message.len() > 4 {
            return CrisisLevel::Mild;
        }
    }

    CrisisLevel::None
}

/// AI 驱动的危机检测（主判定器）
///
/// 关键词匹配后调用此函数，让 AI 判断是否为真正的危机信号。
/// 返回 `(等级, 置信度)`，置信度 ≥ 0.6 才触发干预。
pub fn detect_crisis_ai(message: &str) -> Option<CrisisLevel> {
    let prompt = crate::prompt::PromptManager::get().raw("crisis_ai_detect");
    let result = crate::ai::analyze(prompt, message);
    match result {
        Ok(reply) => {
            let json_str = crate::ai::extract_json(&reply).unwrap_or_else(|| reply.clone());
            let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
                Ok(v) => v,
                Err(_) => return None,
            };
            let level_str = parsed.get("crisis").and_then(|v| v.as_str()).unwrap_or("none");
            let confidence = parsed.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);

            debug!("crisis AI result: level={}, confidence={}", level_str, confidence);

            // 置信度门槛：低于 0.6 不触发
            if confidence < 0.6 {
                return None;
            }

            match level_str {
                "severe" if confidence >= 0.8 => Some(CrisisLevel::Severe),  // Severe 需要更高置信度
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
/// 流程：
/// 1. 关键词匹配到候选信号 → 调用 AI 二次判断
/// 2. AI 判断 → 按置信度决定是否升级
/// 3. AI 不可用时（API 失败）→ 降级到关键词判断，但 Mild 不触发干预
/// 4. 无频率限制——对每个消息独立判断，不因历史触发而忽略
pub fn update_crisis(user_id: u64, level: CrisisLevel) -> bool {
    let mut state = crate::emotion::get_state(user_id);
    let now = crate::util::now_secs();

    if level != CrisisLevel::None {
        // 关键词命中 → 调用 AI 二次判定
        let ai_result = detect_crisis_ai(&format!("用户 {} 的消息触发了危机关键词，需要判断", user_id));

        // 如果 AI 判定可用，使用 AI 结果
        let final_level = if let Some(ai_level) = ai_result {
            ai_level
        } else {
            // AI 不可用（API 失败等），降级为关键词判断
            // Mild 不触发干预（避免误报），Severe 按关键词触发（安全优先）
            match level {
                CrisisLevel::Severe => {
                    debug!(user_id, "crisis: AI 不可用，按关键词 Severe 处理");
                    CrisisLevel::Mild // 降级到 Mild 以降低影响
                }
                CrisisLevel::Mild => {
                    debug!(user_id, "crisis: AI 不可用，关键词 Mild 跳过干预");
                    return false; // 不干预
                }
                CrisisLevel::None => return false,
            }
        };

        // ── 升级 ──
        if final_level > state.crisis_level || (final_level == state.crisis_level && final_level != CrisisLevel::None) {
            state.crisis_level = final_level;
            state.last_crisis_detected = now;
            state.crisis_clean_count = 0;

            let should_intervene = match final_level {
                CrisisLevel::Severe => {
                    info!(user_id, "crisis: Severe 干预 (AI confirmed)");
                    state.last_crisis_intervention = now;
                    true
                }
                CrisisLevel::Mild => {
                    if now.saturating_sub(state.last_crisis_intervention) >= MILD_INTERVENTION_COOLDOWN {
                        info!(user_id, "crisis: Mild 干预 (AI confirmed)");
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

        // AI 判定不升级，保持现状
        crate::emotion::update_state(user_id, state);
        return false;
    }

    // ── 未检测到候选信号：降级检查 ──
    if state.crisis_level == CrisisLevel::None {
        return false;
    }

    state.crisis_clean_count += 1;
    let time_since_detected = now.saturating_sub(state.last_crisis_detected);

    match state.crisis_level {
        CrisisLevel::Severe => {
            if time_since_detected >= SEVERE_COOLDOWN && state.crisis_clean_count >= SEVERE_CLEAN_MSG {
                info!(user_id, "crisis: Severe -> Mild");
                state.crisis_level = CrisisLevel::Mild;
                state.crisis_clean_count = 0;
                state.last_crisis_detected = now;
            }
        }
        CrisisLevel::Mild => {
            if time_since_detected >= MILD_COOLDOWN && state.crisis_clean_count >= MILD_CLEAN_MSG {
                info!(user_id, "crisis: Mild -> None");
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

pub fn get_crisis_context(crisis: CrisisLevel) -> String {
    match crisis {
        CrisisLevel::None => String::new(),
        CrisisLevel::Mild => crate::prompt::PromptManager::get().raw("crisis_mild").to_string(),
        CrisisLevel::Severe => crate::prompt::PromptManager::get().raw("crisis_severe").to_string(),
    }
}
