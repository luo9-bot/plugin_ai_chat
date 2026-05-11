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

/// 群级消息去重：记录每个群最近发送的消息 (group_id -> (message, timestamp))
static RECENT_GROUP_MESSAGES: Mutex<Option<HashMap<u64, (String, u64)>>> = Mutex::new(None);

/// 同群相同消息的冷却时间（秒）
const GROUP_MSG_COOLDOWN_SECS: u64 = 600; // 10 分钟

/// 检查同群是否 recently 发过相同消息
fn is_duplicate_message(group_id: u64, msg: &str) -> bool {
    let guard = RECENT_GROUP_MESSAGES.lock().unwrap();
    if let Some(ref map) = *guard {
        if let Some((recent_msg, recent_time)) = map.get(&group_id) {
            let now = crate::util::now_secs();
            if now.saturating_sub(*recent_time) < GROUP_MSG_COOLDOWN_SECS && recent_msg == msg {
                return true;
            }
        }
    }
    false
}

/// 记录群级已发送消息
fn record_group_message(group_id: u64, msg: &str) {
    let mut guard = RECENT_GROUP_MESSAGES.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    map.insert(group_id, (msg.to_string(), crate::util::now_secs()));
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
            record_sent(user_id);
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

    let emo = emotion::get_state(user_id);

    // ── 触发路径 1: 情绪冲动 ────────────────────────────────────
    // 情绪强烈时，不等固定间隔，随机概率触发
    if time_since_last > 600 && time_since_reply > 600 {
        let impulse_prob = mood_impulse_probability(&emo);
        let roll = pseudo_random(user_id.wrapping_add(now));
        if impulse_prob > 0.0 && roll < impulse_prob {
            let msg = generate_mood_message(user_id, &emo, group_id);
            if is_duplicate_message(group_id, &msg) {
                debug!(user_id, group_id, msg = %msg, "proactive: duplicate mood message, skipping");
                return;
            }
            debug!(user_id, group_id, msg = %msg, emotion = ?emo.current, intensity = emo.intensity, roll, "proactive: mood impulse");
            if sender::safe_send_quiet(group_id, user_id, &msg) {
                record_sent(user_id);
                record_group_message(group_id, &msg);
            }
            return;
        }
    }

    // ── 触发路径 2: 随机化间隔的时间问候 ────────────────────────
    // 把固定间隔乘以一个 0.5~1.5 的随机因子
    let jitter = 0.5 + pseudo_random(user_id.wrapping_add(now / 60)) * 1.0;
    let effective_interval = (interval as f64 * jitter) as u64;

    // 低情绪时延长间隔
    let mood_adjusted = if emo.current == emotion::EmotionType::Sad || emo.current == emotion::EmotionType::Tired {
        (effective_interval as f64 * low_mood_mult) as u64
    } else {
        effective_interval
    };

    if time_since_last >= mood_adjusted && time_since_reply >= mood_adjusted {
        let msg = generate_greeting(user_id, group_id);
        if is_duplicate_message(group_id, &msg) {
            debug!(user_id, group_id, msg = %msg, "proactive: duplicate greeting, skipping");
            return;
        }
        debug!(user_id, group_id, msg = %msg, time_since_last, time_since_reply, jitter = format!("{:.2}", jitter), "proactive: sending greeting");
        if sender::safe_send_quiet(group_id, user_id, &msg) {
            record_sent(user_id);
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

/// 群聊氛围评估：per-group 的主动参与
///
/// 不再对群里每个人独立检查，而是评估整个群的氛围：
/// 1. 群聊是否安静了一段时间
/// 2. bot 是否有活跃的活动状态（训练、吃饭等）
/// 3. 情绪是否波动
/// 4. 是否有特殊日期
pub fn check_group_atmosphere(group_id: u64) {
    let (enabled, quiet_start, quiet_end, _, _, _) = effective_config();
    if !enabled {
        return;
    }

    if is_quiet_hour(quiet_start, quiet_end) {
        return;
    }

    let now = crate::util::now_secs();

    // 检查 bot 是否有活跃的活动状态（训练、吃饭等）
    // 活动状态下不主动参与群聊
    let self_qq = crate::config::get().self_qq;
    if self_qq > 0 {
        if let Some(activity) = crate::activity::get_active_activity(self_qq) {
            debug!(group_id, activity = ?activity.activity, "proactive: bot is busy, skipping");
            return;
        }
    }

    // 检查群聊最后活跃时间
    let last_conversation = crate::with_shared_state(|s| {
        s.last_conversation_times.get(&group_id).copied().unwrap_or(0)
    });
    let quiet_duration = now.saturating_sub(last_conversation);

    // 群聊安静超过 10 分钟才考虑参与
    if quiet_duration < 600 {
        debug!(group_id, quiet_duration, "proactive: group not quiet enough");
        return;
    }

    // 检查 bot 最近是否在群里发过消息
    let recent_bot_msgs = crate::read_shared_state(|s| {
        s.get_recent_bot_messages(group_id, 600, 3)
    });
    if !recent_bot_msgs.is_empty() {
        debug!(group_id, "proactive: bot recently sent messages, skipping");
        return;
    }

    // 检查同群消息去重
    let msg = generate_atmosphere_message(group_id);
    if msg.is_empty() {
        return;
    }

    if is_duplicate_message(group_id, &msg) {
        debug!(group_id, msg = %msg, "proactive: duplicate atmosphere message, skipping");
        return;
    }

    // 发送氛围消息
    info!(group_id, msg = %msg, quiet_duration, "proactive: atmosphere participation");
    sender::safe_send_quiet(group_id, 0, &msg);
    record_group_message(group_id, &msg);
}

/// 生成氛围消息（AI 生成，失败返回空）
fn generate_atmosphere_message(group_id: u64) -> String {
    let emo = emotion::get_state(crate::config::get().self_qq);
    let trigger = if emo.intensity > 0.7 {
        "mood_impulse"
    } else {
        "group_atmosphere"
    };
    super::generate::ai_generate_message(trigger, 0, group_id, &emo).unwrap_or_default()
}

/// 情绪冲动概率: 情绪越强烈，越可能突然想说话
fn mood_impulse_probability(emo: &emotion::EmotionState) -> f64 {
    match emo.current {
        // 开心/兴奋: 有话想说
        emotion::EmotionType::Happy | emotion::EmotionType::Excited => {
            (emo.intensity as f64 * 0.15).min(0.12)
        }
        // 难过/担心: 想找人倾诉
        emotion::EmotionType::Sad | emotion::EmotionType::Worried => {
            (emo.intensity as f64 * 0.12).min(0.10)
        }
        // 惊讶: 忍不住想说
        emotion::EmotionType::Surprised => {
            (emo.intensity as f64 * 0.18).min(0.15)
        }
        // 生气: 想吐槽
        emotion::EmotionType::Angry => {
            (emo.intensity as f64 * 0.08).min(0.06)
        }
        // 害羞: 不太会主动
        emotion::EmotionType::Shy => 0.0,
        // 思考/疲惫/中性: 低概率
        _ => 0.01,
    }
}
