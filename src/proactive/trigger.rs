use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info};

use crate::emotion;
use crate::sender;

use super::runtime::{
    effective_config, load_state, pseudo_random, record_sent,
    is_quiet_hour, check_date_reminders,
};
use super::generate::{generate_mood_message, generate_greeting};

/// 群级最近发送的消息列表（用于内容去重）
/// group_id -> Vec<(content_fingerprint, timestamp)>
static RECENT_GROUP_MESSAGES: Mutex<Option<HashMap<u64, Vec<(String, u64)>>>> = Mutex::new(None);

/// 同群去重冷却时间（秒）
const GROUP_MSG_COOLDOWN_SECS: u64 = 1200;

/// 保留的最近消息数量
const RECENT_MSG_KEEP: usize = 5;

/// 提取消息的"指纹"：取前 8 个非空白字符用于去重
fn content_fingerprint(msg: &str) -> String {
    let cleaned: String = msg.chars()
        .filter(|c| !c.is_whitespace() && *c != '，' && *c != '。' && *c != '~' && *c != '|')
        .collect();
    cleaned.chars().take(8).collect()
}

/// 检查消息是否与近期发送过的消息相似
///
/// 两条消息只要有相同的前 8 个有效字符即视为重复
fn is_duplicate_message(group_id: u64, msg: &str) -> bool {
    let fp = content_fingerprint(msg);
    if fp.is_empty() {
        return false;
    }
    let guard = RECENT_GROUP_MESSAGES.lock().unwrap();
    if let Some(ref map) = *guard
        && let Some(msgs) = map.get(&group_id)
    {
        let now = crate::util::now_secs();
        for (recent_fp, recent_time) in msgs {
            if now.saturating_sub(*recent_time) < GROUP_MSG_COOLDOWN_SECS && recent_fp == &fp {
                return true;
            }
        }
    }
    false
}

/// 记录已发送消息的指纹
fn record_group_message(group_id: u64, msg: &str) {
    let fp = content_fingerprint(msg);
    if fp.is_empty() {
        return;
    }
    let mut guard = RECENT_GROUP_MESSAGES.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    let msgs = map.entry(group_id).or_default();
    msgs.push((fp, crate::util::now_secs()));
    if msgs.len() > RECENT_MSG_KEEP {
        msgs.remove(0);
    }
}

pub fn check_proactive_messages(user_id: u64, group_id: u64) {
    info!(user_id, group_id, "proactive: 检查主动消息");
    let (enabled, quiet_start, quiet_end, interval, max_ignore, low_mood_mult) = effective_config();
    if !enabled {
        return;
    }

    let state = load_state(user_id);
    let now = crate::util::now_secs();

    if is_quiet_hour(quiet_start, quiet_end) {
        tracing::debug!(user_id, group_id, "proactive: quiet hour, skipping");
        return;
    }

    // 日期提醒 -- 最高优先级
    if let Some(reminder_msg) = check_date_reminders(user_id, &state) {
        if is_duplicate_message(group_id, &reminder_msg) {
            debug!(user_id, group_id, msg = %reminder_msg, "proactive: duplicate date reminder, skipping");
            return;
        }
        debug!(user_id, group_id, msg = %reminder_msg, "proactive: sending date reminder");
        if sender::safe_send_quiet(group_id, user_id, &reminder_msg) {
            record_sent(user_id, group_id);
            record_group_message(group_id, &reminder_msg);
        }
        return;
    }

    let time_since_last = now.saturating_sub(state.last_sent);
    let time_since_reply = now.saturating_sub(state.last_user_reply);

    // 最低保底: 被无视太多次就别再烦了
    if state.ignore_count >= max_ignore {
        let cooldown = interval * (state.ignore_count as u64);
        if time_since_last < cooldown {
            tracing::debug!(user_id, group_id, ignore_count = state.ignore_count, cooldown, "proactive: cooling down");
            return;
        }
    }

    // 无新消息不触发：如果工作记忆没有更新，说明群聊没有新内容，不应该重复触发
    if group_id > 0 && state.last_working_memory_ts > 0 {
        let latest_ts = crate::working_memory::get_latest_user_message_ts(group_id);
        if latest_ts <= state.last_working_memory_ts {
            tracing::debug!(user_id, group_id, last_ts = state.last_working_memory_ts, latest_ts, "proactive: no new messages since last send, skipping");
            return;
        }
    }

    let emo = emotion::get_state(user_id);

    // ── 触发路径 1: 情绪冲动 ────────────────────────────────────
    // 情绪强烈时，不等固定间隔，随机概率触发
    // 但最低也要超过 interval/2，防止太随意
    let min_impulse_wait = (interval / 2).max(600);
    if time_since_last > min_impulse_wait && time_since_reply > min_impulse_wait {
        let impulse_prob = mood_impulse_probability(&emo);
        let roll = pseudo_random(user_id.wrapping_add(now));
        if impulse_prob > 0.0 && roll < impulse_prob {
            let msg = generate_mood_message(user_id, &emo, group_id);
            if msg.is_empty() {
                return;
            }
            if is_duplicate_message(group_id, &msg) {
                debug!(user_id, group_id, msg = %msg, "proactive: duplicate mood message, skipping");
                return;
            }
            debug!(user_id, group_id, msg = %msg, emotion = ?emo.current, intensity = emo.intensity, roll, "proactive: mood impulse");
            if sender::safe_send_quiet(group_id, user_id, &msg) {
                record_sent(user_id, group_id);
                record_group_message(group_id, &msg);
            }
            return;
        }
    }

    // ── 触发路径 2: 随机化间隔的主动消息 ────────────────────────
    let jitter = 0.5 + pseudo_random(user_id.wrapping_add(now / 60)) * 1.0;
    let effective_interval = (interval as f64 * jitter) as u64;

    let mood_adjusted = if emo.current == emotion::EmotionType::Sad || emo.current == emotion::EmotionType::Tired {
        (effective_interval as f64 * low_mood_mult) as u64
    } else {
        effective_interval
    };

    if time_since_last >= mood_adjusted && time_since_reply >= mood_adjusted {
        let msg = generate_greeting(user_id, group_id);
        if msg.is_empty() {
            return;
        }
        if is_duplicate_message(group_id, &msg) {
            debug!(user_id, group_id, msg = %msg, time_since_last, "proactive: duplicate, skipping");
            return;
        }
        debug!(user_id, group_id, msg = %msg, time_since_last, time_since_reply, jitter = format!("{:.2}", jitter), "proactive: sending");
        if sender::safe_send_quiet(group_id, user_id, &msg) {
            record_sent(user_id, group_id);
            record_group_message(group_id, &msg);
        }
    } else {
        tracing::debug!(
            user_id, group_id,
            time_since_last,
            time_since_reply,
            ignore_count = state.ignore_count,
            effective_interval,
            jitter = format!("{:.2}", jitter),
            emotion = ?emo.current,
            "proactive: not yet time"
        );
    }
}

/// 群聊氛围评估（已不再被 check_periodic 调用，保留供外部扩展用）
pub fn check_group_atmosphere(group_id: u64) {
    let (enabled, quiet_start, quiet_end, _, _, _) = effective_config();
    if !enabled {
        return;
    }

    if is_quiet_hour(quiet_start, quiet_end) {
        return;
    }

    let now = crate::util::now_secs();

    let self_qq = crate::config::get().self_qq;
    if self_qq > 0
        && let Some(activity) = crate::activity::get_active_activity(self_qq) {
            debug!(group_id, activity = ?activity.activity, "proactive: bot is busy, skipping");
            return;
        }

    let last_conversation = crate::with_shared_state(|s| {
        s.last_conversation_times.get(&group_id).copied().unwrap_or(0)
    });
    let quiet_duration = now.saturating_sub(last_conversation);

    if quiet_duration < 600 {
        debug!(group_id, quiet_duration, "proactive: group not quiet enough");
        return;
    }

    let recent_bot_msgs = crate::read_shared_state(|s| {
        s.get_recent_bot_messages(group_id, 600, 3)
    });
    if !recent_bot_msgs.is_empty() {
        debug!(group_id, "proactive: bot recently sent messages, skipping");
        return;
    }

    let msg = generate_atmosphere_message(group_id);
    if msg.is_empty() {
        return;
    }

    if is_duplicate_message(group_id, &msg) {
        debug!(group_id, msg = %msg, "proactive: duplicate atmosphere message, skipping");
        return;
    }

    info!(group_id, msg = %msg, quiet_duration, "proactive: atmosphere participation");
    sender::safe_send_quiet(group_id, 0, &msg);
    record_group_message(group_id, &msg);
}

/// 生成氛围消息
fn generate_atmosphere_message(group_id: u64) -> String {
    let emo = emotion::get_state(crate::config::get().self_qq);
    let trigger = if emo.intensity > 0.7 {
        "mood_impulse"
    } else {
        "group_atmosphere"
    };
    super::generate::ai_generate_message(trigger, 0, group_id, &emo).unwrap_or_default()
}

/// 情绪冲动概率: 大幅降低，只有强烈情绪才可能触发
fn mood_impulse_probability(emo: &emotion::EmotionState) -> f64 {
    match emo.current {
        emotion::EmotionType::Happy | emotion::EmotionType::Excited => {
            (emo.intensity as f64 * 0.06).min(0.04)
        }
        emotion::EmotionType::Sad | emotion::EmotionType::Worried => {
            (emo.intensity as f64 * 0.05).min(0.03)
        }
        emotion::EmotionType::Surprised => {
            (emo.intensity as f64 * 0.08).min(0.05)
        }
        emotion::EmotionType::Angry => {
            (emo.intensity as f64 * 0.03).min(0.02)
        }
        emotion::EmotionType::Shy => 0.0,
        _ => 0.003,
    }
}
