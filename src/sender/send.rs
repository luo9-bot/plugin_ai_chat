//! 消息发送：底层发送、打字延迟、安全检查

use luo9_sdk::Bot;
use std::ffi::CString;
use std::thread;
use std::time::Duration;
use tracing::{info, warn};

use super::segments::{normalize_segment_sep, split_segments, clean_reply};
use super::timing::ResponseTiming;
use crate::anti_injection;
use crate::config;

/// 底层发送：发送单条原始消息（不分割、无延迟）
fn raw_send_msg(group_id: u64, user_id: u64, text: &str) {
    if group_id > 0 {
        info!(group_id, user_id, content = text, "send: group msg");
        let msg = CString::new(text).unwrap();
        Bot::send_group_msg(group_id, msg);
        crate::working_memory::record_bot_reply(group_id, text);
    } else {
        info!(user_id, content = text, "send: private msg");
        let msg = CString::new(text).unwrap();
        Bot::send_private_msg(user_id, msg);
    }
}

/// 发送消息，带打字模拟延迟
pub fn send_with_typing(group_id: u64, user_id: u64, reply: &str) {
    let cfg = config::get();

    // 构建动态回复时机
    let mut timing = ResponseTiming::default();

    // 收集当前状态以调整速度修正系数
    if cfg.humanity.response_timing_enabled {
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
    }

    // 检查是否应该拆分回复（"先发简短反应，再发完整内容"）
    let reply_text = if cfg.humanity.response_timing_enabled && timing.should_split_reply(reply) {
        if let Some((first, second)) = ResponseTiming::split_reply(reply) {
            // 先发第一部分
            let delay = timing.calculate_delay(&first);
            if delay > 0 {
                thread::sleep(Duration::from_millis(delay));
            }
            raw_send_msg(group_id, user_id, &first);

            // 短暂停顿后发第二部分
            thread::sleep(Duration::from_millis(800 + fastrand::u64(200..1500)));

            // 第二部分作为主要回复内容
            second
        } else {
            reply.to_string()
        }
    } else {
        reply.to_string()
    };

    let normalized = normalize_segment_sep(&reply_text);
    let parts = split_segments(&normalized);

    for (i, text) in parts.iter().enumerate() {
        if i == 0 {
            // 第一条消息前应用思考延迟
            let delay = timing.calculate_delay(text);
            if delay > 0 {
                thread::sleep(Duration::from_millis(delay.min(2000))); // 首条消息思考延迟最多2秒
            }
        }

        raw_send_msg(group_id, user_id, text);

        if i < parts.len() - 1 {
            let delay = timing.calculate_delay(text);
            let delay_ms = delay.min(cfg.conversation.max_typing_delay_ms);
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

/// 发送带 @ 的群消息
pub fn send_at_msg(group_id: u64, user_id: u64, text: &str) {
    let full = format!("[CQ:at,qq={}]\n{}", user_id, text);
    info!(group_id, user_id, content = text, "send: at msg");
    let msg = CString::new(full).unwrap();
    Bot::send_group_msg(group_id, msg);
}

/// 安全发送 AI 生成的消息：clean_reply + check_output + 分割 + 打字延迟
pub fn safe_send(group_id: u64, user_id: u64, reply: &str) -> bool {
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
            send_with_typing(group_id, user_id, &sanitized);
        }
        return false;
    }
    send_with_typing(group_id, user_id, &cleaned);
    true
}

/// 安静版安全发送：无打字延迟
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
            send_msg(group_id, user_id, &sanitized);
        }
        return false;
    }
    send_msg(group_id, user_id, &cleaned);
    true
}
