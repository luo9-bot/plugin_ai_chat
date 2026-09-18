//! 消息发送：底层发送、打字延迟、安全检查
//!
//! 分段由她自己的输出决定（|^| 和换行），发送端只负责节奏：
//! 打字速度受电量/节律/注意力影响，首条消息前有思考延迟，
//! 段间隔按下一条的字数模拟"先把下一条打完再发"的真人节奏。
//!
//! 节奏等待必须发生在**决策之外**：群聊批次由一个串行队列线程处理，
//! 如果发送时在那里 `sleep` 打字时间，同一个群里后到的消息就要排队
//! 等她"打完字"——实测用户消息要等 5~12 秒才被看见。因此对外暴露的
//! `safe_send` 只同步做安全检查，真正的发送（含等待）交给独立线程。

use luo9_sdk::Bot;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::Duration;
use tracing::{info, warn};

use super::segments::{clean_reply, normalize_segment_sep, split_segments};
use super::timing::ResponseTiming;
use crate::anti_injection;
use crate::config;

/// 每条会话（群或私聊）的发送锁
///
/// 发送现在跑在独立线程里，而"她的一次回复"可能分成几条气泡、
/// 泡与泡之间还有打字间隔。没有这把锁，两轮回复的线程会在间隔里
/// 交错，群里看到的是甲句、乙句、甲句。锁只保护发送序列本身——
/// 决策线程从不等待它。
fn send_locks() -> &'static Mutex<HashMap<u64, &'static Mutex<()>>> {
    static LOCKS: OnceLock<Mutex<HashMap<u64, &'static Mutex<()>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn conversation_key(group_id: u64, user_id: u64) -> u64 {
    if group_id > 0 { group_id } else { user_id }
}

/// 取得该会话的发送锁；等待时打印一次告警（正常情况不该等）
fn lock_conversation(key: u64) -> MutexGuard<'static, ()> {
    let cell: &'static Mutex<()> = {
        let mut locks = send_locks().lock().unwrap_or_else(|e| e.into_inner());
        locks
            .entry(key)
            .or_insert_with(|| Box::leak(Box::new(Mutex::new(()))))
    };
    match cell.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            warn!(key, "sender: 上一条还在发，等它发完");
            cell.lock().unwrap_or_else(|e| e.into_inner())
        }
    }
}

/// 底层发送：发送单条原始消息（不分割、无延迟）
fn raw_send_msg(group_id: u64, user_id: u64, text: &str) {
    if group_id > 0 {
        info!(group_id, user_id, content = text, "send: group msg");
        let msg = crate::util::to_c_string(text);
        Bot::send_group_msg(group_id, msg);
        crate::working_memory::record_bot_reply(group_id, text);
    } else {
        info!(user_id, content = text, "send: private msg");
        let msg = crate::util::to_c_string(text);
        Bot::send_private_msg(user_id, msg);
    }
}

/// 按她的状态构建当前打字节奏
fn current_timing() -> ResponseTiming {
    let cfg = config::get();
    let mut timing = ResponseTiming::default();
    if !cfg.humanity.response_timing_enabled {
        return timing;
    }
    let battery_level = if cfg.humanity.social_battery_enabled {
        crate::social_battery::load().level / cfg.humanity.battery_capacity
    } else {
        0.7
    };
    let circadian_energy = if cfg.humanity.circadian_enabled {
        crate::circadian::get_energy_multiplier()
    } else {
        0.7
    };
    let attention_level = if cfg.humanity.attention_enabled {
        crate::conversation::attention::load_attention().attention_level
    } else {
        0.7
    };
    timing.update_modifiers(battery_level, circadian_energy, attention_level);
    timing
}

/// 发送消息，带打字模拟延迟（阻塞当前线程）
///
/// `incoming` 是这条回复在回应的原文——用来估算"读完对方的话"的时间。
pub fn send_with_typing(group_id: u64, user_id: u64, reply: &str, incoming: &str) {
    let _guard = lock_conversation(conversation_key(group_id, user_id));
    let cfg = config::get();
    let timing = current_timing();

    let normalized = normalize_segment_sep(reply);
    let parts = split_segments(&normalized);
    let cap = cfg.conversation.max_typing_delay_ms;

    for (i, text) in parts.iter().enumerate() {
        if i == 0 {
            // 第一条消息前应用思考延迟
            let delay = timing.calculate_delay(text, incoming, cap);
            if delay > 0 {
                thread::sleep(Duration::from_millis(delay.min(cap)));
            }
        }

        raw_send_msg(group_id, user_id, text);

        // 段间隔 = 打下一条消息所需的时间：真人要先把下一条打完才发出来
        if let Some(next) = parts.get(i + 1) {
            let delay_ms = timing.calculate_delay(next, "", cap).min(cap);
            if delay_ms > 0 {
                thread::sleep(Duration::from_millis(delay_ms));
            }
        }
    }
}

/// 发送消息 (无延迟)，自动处理 |^| 和换行分割
pub fn send_msg(group_id: u64, user_id: u64, text: &str) {
    let normalized = normalize_segment_sep(text);
    let segments = split_segments(&normalized);
    for segment in &segments {
        raw_send_msg(group_id, user_id, segment);
    }
}

/// 发送消息，分段之间带随机节奏（用于主动消息，避免机械同秒连发）
fn send_msg_rhythmic(group_id: u64, user_id: u64, text: &str) {
    let _guard = lock_conversation(conversation_key(group_id, user_id));
    let normalized = normalize_segment_sep(text);
    let segments = split_segments(&normalized);
    for (i, segment) in segments.iter().enumerate() {
        raw_send_msg(group_id, user_id, segment);
        if i < segments.len() - 1 {
            let delay_ms = 600 + fastrand::u64(0..1200);
            thread::sleep(Duration::from_millis(delay_ms));
        }
    }
}

/// 发送带 @ 的群消息
pub fn send_at_msg(group_id: u64, user_id: u64, text: &str) {
    let full = format!("[CQ:at,qq={}]\n{}", user_id, text);
    info!(group_id, user_id, content = text, "send: at msg");
    let msg = crate::util::to_c_string(full);
    Bot::send_group_msg(group_id, msg);
}

/// 安全发送 AI 生成的消息：clean_reply + check_output + 分割 + 打字延迟
///
/// `incoming` 是她正在回应的原文（用于估算阅读时间）；传空串表示
/// 没有具体原文（主动消息、表情包后的补话）。
///
/// **本函数不阻塞**：安全检查在这里同步做完，节奏等待与实际发送交给
/// 独立线程。这样群聊的串行决策线程不会因为"她正在打字"而卡住，
/// 后到的消息能立刻进入下一轮判断。返回 false 表示内容被安全系统拦截。
pub fn safe_send(group_id: u64, user_id: u64, reply: &str, incoming: &str) -> bool {
    let cleaned = clean_reply(reply);
    let cfg = config::get();
    let check = anti_injection::check_output(user_id, &cleaned, &cfg.anti_injection);
    if !check.passed {
        warn!(
            user_id, group_id,
            issues = ?check.issues,
            action = ?check.action,
            "sender: AI 消息被安全系统拦截"
        );
        if let Some(sanitized) = check.sanitized {
            spawn_typed_send(group_id, user_id, sanitized, incoming.to_string());
        }
        return false;
    }
    spawn_typed_send(group_id, user_id, cleaned, incoming.to_string());
    true
}

/// 在独立线程里按她的节奏把消息发出去
fn spawn_typed_send(group_id: u64, user_id: u64, reply: String, incoming: String) {
    thread::spawn(move || {
        send_with_typing(group_id, user_id, &reply, &incoming);
    });
}

/// 安静版安全发送：无打字延迟，同样不阻塞调用方
pub fn safe_send_quiet(group_id: u64, user_id: u64, reply: &str) -> bool {
    let cleaned = clean_reply(reply);
    let cfg = config::get();
    let check = anti_injection::check_output(user_id, &cleaned, &cfg.anti_injection);
    if !check.passed {
        warn!(
            user_id, group_id,
            issues = ?check.issues,
            action = ?check.action,
            "sender: AI 消息被安全系统拦截"
        );
        if let Some(sanitized) = check.sanitized {
            thread::spawn(move || send_msg_rhythmic(group_id, user_id, &sanitized));
        }
        return false;
    }
    thread::spawn(move || send_msg_rhythmic(group_id, user_id, &cleaned));
    true
}
