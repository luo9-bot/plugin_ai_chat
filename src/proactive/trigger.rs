use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info};

use crate::emotion;
use crate::sender;
use crate::with_shared_state;

use super::runtime::{
    effective_config, load_state, pseudo_random, record_sent,
    is_quiet_hour, check_date_reminders,
};
use super::generate::{generate_mood_message, generate_greeting};
use super::motivation;

/// Bot 主动消息的回复状态
#[derive(Debug, Clone, PartialEq)]
pub enum ReplyStatus {
    /// 有人回复了 bot 的消息（面向 bot）
    RepliedToBot,
    /// 有人在聊天但不是回复 bot
    OthersTalking,
    /// 没人回复
    NoReply,
}

/// 群级最近发送的消息列表（用于内容去重）
/// group_id -> Vec<(content_fingerprint, timestamp)>
static RECENT_GROUP_MESSAGES: Mutex<Option<HashMap<u64, Vec<(String, u64)>>>> = Mutex::new(None);

/// 同群去重冷却时间（秒）
const GROUP_MSG_COOLDOWN_SECS: u64 = 3600;

/// 保留的最近消息数量
const RECENT_MSG_KEEP: usize = 15;

/// 提取消息的"指纹"：取前 12 个非空白字符用于去重
fn content_fingerprint(msg: &str) -> String {
    let cleaned: String = msg.chars()
        .filter(|c| !c.is_whitespace() && *c != '，' && *c != '。' && *c != '~' && *c != '|' && *c != '^')
        .collect();
    cleaned.chars().take(12).collect()
}

/// 检查两条指纹是否相似
///
/// 策略：
/// 1. 完全相同 → 重复
/// 2. 互相包含 → 重复
/// 3. 字符级重叠度 >= 60% → 重复（防"豆还在播呢"≈"豆还在播啊"）
fn fingerprints_similar(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // 完全相同或互相包含
    if a == b || a.contains(b) || b.contains(a) {
        return true;
    }
    // 字符级重叠度检查：短指纹适合字符级别，长指纹做 bigram
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let short_chars: std::collections::HashSet<char> = short.chars().collect();
    let shared = long.chars().filter(|c| short_chars.contains(c)).count();
    let overlap_ratio = shared as f64 / long.chars().count() as f64;
    overlap_ratio >= 0.6
}

/// 消息话题分类（AI 驱动）
fn classify_message_topic(msg: &str) -> String {
    let clean: String = msg.chars()
        .filter(|c| !c.is_whitespace() && *c != '|' && *c != '^')
        .collect();
    if clean.is_empty() {
        return "other".to_string();
    }

    // 快速规则首 pass：明显的话题直接返回，避免 AI 调用
    if clean.contains("早") || clean.contains("晚安") || clean.contains("你好") {
        return "greeting".to_string();
    }

    // AI 分类
    let prompt = crate::prompt::PromptManager::get().raw("topic_classify");
    let user_content = format!("请分类这条消息：\n{}", clean);
    match crate::ai::analyze_with_tools(
        prompt,
        &user_content,
        &[crate::ai::topic_classify_tool()],
        Some(serde_json::json!("auto")),
    ) {
        Ok(parsed) => {
            parsed.get("topic")
                .and_then(|v| v.as_str())
                .unwrap_or("other")
                .to_string()
        }
        Err(_) => "other".to_string(),
    }
}

/// 检查 bot 最近的主动消息是否得到了回复
///
/// 人类逻辑：
/// - 我说了话，有人回我 → 可以继续聊
/// - 我说了话，别人在聊天但没理我 → 不要插嘴
/// - 我说了话，没人理 → 闭嘴
pub fn check_reply_status(group_id: u64) -> (ReplyStatus, Option<String>) {
    let self_qq = crate::config::get().self_qq;
    if self_qq == 0 { return (ReplyStatus::NoReply, None); }

    let history = crate::read_shared_state(|s| s.get_history_clone(group_id, 0));

    // 从后往前找 bot 的最后一条消息
    let mut last_bot_idx: Option<usize> = None;
    let mut last_bot_content = String::new();
    for (i, (role, content)) in history.iter().enumerate().rev() {
        if role == "assistant" {
            last_bot_idx = Some(i);
            last_bot_content = content.clone();
            break;
        }
    }

    let bot_idx = match last_bot_idx {
        Some(i) => i,
        None => return (ReplyStatus::NoReply, None),
    };

    // 收集 bot 消息之后的所有用户消息
    let after_bot: Vec<&(String, String)> = history.iter().skip(bot_idx + 1).collect();

    if after_bot.is_empty() {
        return (ReplyStatus::NoReply, Some(classify_message_topic(&last_bot_content)));
    }

    // 检查是否有用户消息是面向 bot 的
    let bot_name = crate::config::get().bot_name.clone();
    let at_pattern = format!("[CQ:at,qq={}]", self_qq);

    for (role, content) in &after_bot {
        if role != "user" { continue; }
        if content.contains(&at_pattern) || (!bot_name.is_empty() && content.contains(&bot_name)) {
            return (ReplyStatus::RepliedToBot, None);
        }
    }

    // 有用户消息但没有面向 bot → 别人在聊天
    (ReplyStatus::OthersTalking, Some(classify_message_topic(&last_bot_content)))
}

/// 根据回复状态和话题判断是否应该跳过
///
/// 人类逻辑：如果我说了话没人回复，我不会再重复说同类话题。
fn should_skip_by_topic(reply_status: &ReplyStatus, last_topic: &Option<String>, new_msg: &str) -> bool {
    if *reply_status != ReplyStatus::NoReply {
        return false; // 有人回复了或别人在聊，不跳过
    }
    let last = match last_topic {
        Some(t) => t,
        None => return false, // 没有上次话题，不跳过
    };
    let new_topic = classify_message_topic(new_msg);
    if last == "other" || new_topic == "other" {
        return false; // 无法分类的话题不跳过
    }
    if *last == new_topic {
        debug!(topic = %last, "proactive: no reply + same topic, skipping");
        return true;
    }
    false
}

/// 检查消息是否与近期发送过的消息相似
fn is_duplicate_message(group_id: u64, msg: &str) -> bool {
    let fp = content_fingerprint(msg);
    if fp.is_empty() {
        return false;
    }

    // 检查 1: RECENT_GROUP_MESSAGES 缓存（同一群内跨用户去重）
    let guard = RECENT_GROUP_MESSAGES.lock().unwrap();
    if let Some(ref map) = *guard
        && let Some(msgs) = map.get(&group_id)
    {
        let now = crate::util::now_secs();
        for (recent_fp, recent_time) in msgs {
            if now.saturating_sub(*recent_time) < GROUP_MSG_COOLDOWN_SECS && fingerprints_similar(recent_fp, &fp) {
                return true;
            }
        }
    }
    drop(guard);

    // 检查 2: 工作记忆中 bot 自己最近 600 秒的消息
    // 覆盖任何发送路径（对话回复、氛围消息等），跨用户彻底去重
    let self_qq = crate::config::get().self_qq;
    if self_qq > 0 {
        let recent = crate::working_memory::get_recent(group_id, 600, 20);
        for entry in &recent {
            if entry.user_id == self_qq {
                let entry_fp = content_fingerprint(&entry.content);
                if fingerprints_similar(&entry_fp, &fp) {
                    return true;
                }
            }
        }
    }

    // 检查 3: 对话历史中 bot 自己最近 1800 秒的消息（覆盖私聊主动消息）
    if self_qq > 0 {
        let now = crate::util::now_secs();
        let history = crate::read_shared_state(|s| s.get_history_clone(group_id, 0));
        for (role, content) in history.iter().rev().take(10) {
            if role != "assistant" { continue; }
            let entry_fp = content_fingerprint(content);
            if fingerprints_similar(&entry_fp, &fp) {
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

/// 将主动消息写入对话历史，让 AI 知道自己说过什么
fn push_proactive_to_history(group_id: u64, user_id: u64, msg: &str) {
    if user_id == 0 { return; }
    let max_pairs = crate::config::get().conversation.max_history;
    with_shared_state(|s| {
        if group_id > 0 {
            s.push_history(group_id, user_id, "assistant", msg, max_pairs);
            s.push_group_history(group_id, "assistant", msg, max_pairs);
        } else {
            // 私聊：写入用户自己的历史
            s.push_history(0, user_id, "assistant", msg, max_pairs);
        }
    });
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

    // 群级节流：不管哪个 user_id 触发的，只要这个群最近发过消息就跳过
    let group_sent = crate::proactive::get_group_last_sent(group_id);
    let group_cooldown = interval / 2;
    if group_id > 0 && group_sent > 0 && now.saturating_sub(group_sent) < group_cooldown {
        tracing::debug!(user_id, group_id, elapsed = now.saturating_sub(group_sent), group_cooldown, "proactive: group cooldown, skipping");
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
            push_proactive_to_history(group_id, user_id, &reminder_msg);
        }
        return;
    }

    let time_since_last = now.saturating_sub(state.last_sent);
    let time_since_reply = now.saturating_sub(state.last_user_reply);

    // ── 回复状态检测：bot 最近的消息有没有人回复？ ──
    let (reply_status, last_topic) = check_reply_status(group_id);
    debug!(user_id, group_id, ?reply_status, ?last_topic, "proactive: reply status");

    // 最低保底: 被无视太多次就别再烦了
    if state.ignore_count >= max_ignore {
        let cooldown = interval * (state.ignore_count as u64);
        if time_since_last < cooldown {
            tracing::debug!(user_id, group_id, ignore_count = state.ignore_count, cooldown, "proactive: cooling down");
            return;
        }
    }

    // ── 触发路径 3: 生命事件（起床/入睡/活动完成） ────────────
    // 这个路径不受"无新消息"限制——bot 可以基于自己的生活状态说话
    // 但需要超过最短间隔，防止刚发完又发
    let self_qq = crate::config::get().self_qq;
    let min_life_event_interval = 300u64; // 5 分钟
    if self_qq > 0 && time_since_last > min_life_event_interval && time_since_reply > 60 {
        if let Some(life_event) = crate::activity::get_pending_life_event(self_qq) {
            let trigger = match &life_event {
                crate::activity::LifeEvent::WokeUp => "woke_up",
                crate::activity::LifeEvent::GoingToSleep => "going_to_sleep",
                crate::activity::LifeEvent::ActivityCompleted(_) => "activity_completed",
            };
            let life_emo = emotion::get_state(self_qq);
            let extra_ctx = match &life_event {
                crate::activity::LifeEvent::WokeUp => "你刚睡醒，可以自然地跟群友打招呼，或者说说刚醒时的状态。".to_string(),
                crate::activity::LifeEvent::GoingToSleep => "你准备睡觉了，可以告诉大家一声，或者简单道晚安。".to_string(),
                crate::activity::LifeEvent::ActivityCompleted(act) => {
                    format!("你刚完成了{}，可以跟大家分享一下感受或者自然地聊起来。", act.describe())
                }
            };
            let msg = super::generate::ai_generate_message(trigger, user_id, group_id, &life_emo, &extra_ctx);
            if let Some(msg) = msg {
                if msg.is_empty() || is_duplicate_message(group_id, &msg) {
                    crate::activity::clear_life_event(&life_event);
                    return;
                }
                debug!(user_id, group_id, msg = %msg, ?life_event, "proactive: life event");
                if sender::safe_send_quiet(group_id, user_id, &msg) {
                    record_sent(user_id, group_id);
                    record_group_message(group_id, &msg);
                    push_proactive_to_history(group_id, user_id, &msg);
                }
            }
            crate::activity::clear_life_event(&life_event);
            return;
        }
    }

    // 无新消息不触发（不影响路径 3，但对路径 1/2 起作用）
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
    // 回复状态影响：没人回复+同话题 → skip；别人在聊 → 允许（情绪强烈可插话）
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
            if should_skip_by_topic(&reply_status, &last_topic, &msg) {
                debug!(user_id, group_id, msg = %msg, "proactive: no reply + same topic mood, skipping");
                return;
            }
            debug!(user_id, group_id, msg = %msg, emotion = ?emo.current, intensity = emo.intensity, roll, "proactive: mood impulse");
            if sender::safe_send_quiet(group_id, user_id, &msg) {
                record_sent(user_id, group_id);
                record_group_message(group_id, &msg);
                push_proactive_to_history(group_id, user_id, &msg);
            }
            return;
        }
    }

    // ── 触发路径 2: 动机驱动的主动消息 ────────────────────────
    // 基于内在动机（分享欲、关心欲、好奇心等）决定是否说话
    let min_motivation_wait = (interval / 3).max(300); // 最短 5 分钟或 interval/3
    if time_since_last > min_motivation_wait && time_since_reply > 60 {
        let dominant = motivation::get_dominant_motivation();
        if let Some((motivation_type, strength)) = dominant {
            // 动机强度超过阈值才触发
            let threshold = 0.4;
            if strength >= threshold {
                let motivation_ctx = motivation::get_motivation_context();
                let msg = generate_greeting(user_id, group_id);
                if !msg.is_empty() && !is_duplicate_message(group_id, &msg) && !should_skip_by_topic(&reply_status, &last_topic, &msg) {
                    debug!(user_id, group_id, msg = %msg, motivation = %motivation_type, strength, "proactive: motivation-driven");
                    if sender::safe_send_quiet(group_id, user_id, &msg) {
                        record_sent(user_id, group_id);
                        record_group_message(group_id, &msg);
                        push_proactive_to_history(group_id, user_id, &msg);
                        motivation::consume_expression();
                    }
                    return;
                }
            }
        }
    }

    // ── 触发路径 3: 随机化间隔的主动消息 ────────────────────────
    let jitter = 0.5 + pseudo_random(user_id.wrapping_add(now / 60)) * 1.0;
    let effective_interval = (interval as f64 * jitter) as u64;

    let mood_adjusted = if emo.current == emotion::EmotionType::Sad || emo.current == emotion::EmotionType::Tired {
        (effective_interval as f64 * low_mood_mult) as u64
    } else {
        effective_interval
    };

    if time_since_last >= mood_adjusted && time_since_reply >= mood_adjusted {
        // 路径 3：如果没人回复 bot 的消息，直接跳过（不说同类话题）
        if reply_status == ReplyStatus::NoReply {
            debug!(user_id, group_id, "proactive: no reply to last message, skipping periodic greeting");
            return;
        }
        // 别人在聊天但没回复 bot → 不插嘴
        if reply_status == ReplyStatus::OthersTalking {
            debug!(user_id, group_id, "proactive: others talking, not replying to bot, skipping");
            return;
        }

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
            push_proactive_to_history(group_id, user_id, &msg);
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
    super::generate::ai_generate_message(trigger, 0, group_id, &emo, "").unwrap_or_default()
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
