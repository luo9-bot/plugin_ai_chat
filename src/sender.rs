use luo9_sdk::Bot;
use std::ffi::CString;
use std::thread;
use std::time::Duration;
use crate::config;

/// 消息分段分隔符
const SEGMENT_SEP: &str = "|^|";

/// 发送消息，带打字模拟延迟
///
/// 将 AI 回复按 `|^|` 分割为多条消息，逐条发送
/// 没有 `|^|` 则按自然段落（双换行）分割
pub fn send_with_typing(group_id: u64, user_id: u64, reply: &str) {
    let conv = &config::get().conversation;

    let parts: Vec<&str> = if reply.contains(SEGMENT_SEP) {
        reply.split(SEGMENT_SEP).filter(|s| !s.trim().is_empty()).collect()
    } else {
        reply.split("\n\n").filter(|s| !s.trim().is_empty()).collect()
    };

    for (i, part) in parts.iter().enumerate() {
        let text = part.trim();
        if text.is_empty() {
            continue;
        }

        send_msg(group_id, user_id, text);

        if i < parts.len() - 1 {
            let char_count = text.chars().count();
            let delay_secs = char_count as f64 / conv.typing_speed;
            let delay_ms = (delay_secs * 1000.0) as u64;
            let delay_ms = delay_ms.min(conv.max_typing_delay_ms);
            if delay_ms > 0 {
                thread::sleep(Duration::from_millis(delay_ms));
            }
        }
    }
}

/// 直接发送单条消息 (无延迟)
pub fn send_msg(group_id: u64, user_id: u64, text: &str) {
    let msg = CString::new(text).unwrap();
    if group_id > 0 {
        Bot::send_group_msg(group_id, msg);
    } else {
        Bot::send_private_msg(user_id, msg);
    }
}

/// 发送带 @ 的群消息
pub fn send_at_msg(group_id: u64, user_id: u64, text: &str) {
    let msg = CString::new(format!("[CQ:at,qq={}]\n{}", user_id, text)).unwrap();
    Bot::send_group_msg(group_id, msg);
}
