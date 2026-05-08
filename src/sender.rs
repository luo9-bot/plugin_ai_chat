use luo9_sdk::Bot;
use std::ffi::CString;
use std::thread;
use std::time::Duration;
use tracing::{info, warn};
use crate::anti_injection;
use crate::config;

/// 消息分段分隔符
const SEGMENT_SEP: &str = "|^|";

/// 规范化消息分段分隔符：将 AI 生成的不完整 "|^" 或 "^|" 补全为 "|^|"
/// 同时规范化换行：双换行 → 单换行，确保后续分割行为一致
pub fn normalize_segment_sep(reply: &str) -> String {
    // 先规范化换行：连续换行压缩为单个
    let mut normalized = reply.replace("\r\n", "\n");
    while normalized.contains("\n\n") {
        normalized = normalized.replace("\n\n", "\n");
    }

    if !normalized.contains('^') {
        return normalized;
    }

    // 单次扫描：识别所有 ^ 位置，判断上下文决定是否为分隔符
    let chars: Vec<char> = normalized.chars().collect();
    let len = chars.len();
    let mut is_sep: Vec<bool> = vec![false; len]; // 标记属于分隔符的字符

    for i in 0..len {
        if chars[i] != '^' {
            continue;
        }
        let has_pipe_before = i > 0 && chars[i - 1] == '|';
        let has_pipe_after = i + 1 < len && chars[i + 1] == '|';
        if has_pipe_before || has_pipe_after {
            // 这个 ^ 是分隔符的一部分
            is_sep[i] = true;
            if has_pipe_before {
                is_sep[i - 1] = true;
            }
            if has_pipe_after {
                is_sep[i + 1] = true;
            }
        }
    }

    // 按分隔符组重建字符串：每组都输出为 |^|
    let mut out = String::with_capacity(len + 8);
    let mut i = 0;
    while i < len {
        if is_sep[i] {
            // 跳过整个分隔符组
            while i < len && is_sep[i] {
                i += 1;
            }
            out.push_str("|^|");
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }

    out
}

/// 将已规范化的消息按 `|^|` 和换行分割为最终发送片段
pub fn split_segments(normalized_reply: &str) -> Vec<String> {
    if normalized_reply.contains(SEGMENT_SEP) {
        normalized_reply.split(SEGMENT_SEP)
            .flat_map(|s| s.split('\n'))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        normalized_reply.split('\n')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// 发送消息，带打字模拟延迟
///
/// 将 AI 回复按 `|^|` 和换行分割为多条消息，逐条发送
pub fn send_with_typing(group_id: u64, user_id: u64, reply: &str) {
    let conv = &config::get().conversation;
    let normalized = normalize_segment_sep(reply);
    let parts = split_segments(&normalized);

    for (i, text) in parts.iter().enumerate() {
        raw_send_msg(group_id, user_id, text);

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

/// 底层发送：发送单条原始消息（不分割、无延迟）
fn raw_send_msg(group_id: u64, user_id: u64, text: &str) {
    if group_id > 0 {
        info!(group_id, user_id, content = text, "send: group msg");
        let msg = CString::new(text).unwrap();
        Bot::send_group_msg(group_id, msg);
    } else {
        info!(user_id, content = text, "send: private msg");
        let msg = CString::new(text).unwrap();
        Bot::send_private_msg(user_id, msg);
    }
}

/// 发送消息 (无延迟)，自动处理 |^| 和换行分割
///
/// 如果消息包含 |^| 分隔符或换行，会自动分割为多条消息逐条发送。
/// 静态消息不含 |^|，行为不变。
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

/// 安全发送 AI 生成的消息：check_output + 分割 + 打字延迟
///
/// 所有 AI 生成的消息都应通过此函数发送，确保：
/// 1. 内容经过安全检查（check_output）
/// 2. |^| 分隔符被正确处理并分割为多条消息
/// 3. 带打字延迟模拟
pub fn safe_send(group_id: u64, user_id: u64, reply: &str) -> bool {
    let cfg = config::get();
    let check = anti_injection::check_output(user_id, reply, &cfg.anti_injection);
    if !check.passed {
        warn!(
            user_id, group_id,
            issues = ?check.issues,
            action = ?check.action,
            "sender: AI 消息被安全系统拦截"
        );
        // 发送替换内容（如果有）
        if let Some(sanitized) = check.sanitized {
            send_with_typing(group_id, user_id, &sanitized);
        }
        return false;
    }

    send_with_typing(group_id, user_id, reply);
    true
}

/// 安静版安全发送：无打字延迟，用于主动消息等场景
///
/// check_output + 自动分割，但不模拟打字延迟
pub fn safe_send_quiet(group_id: u64, user_id: u64, reply: &str) -> bool {
    let cfg = config::get();
    let check = anti_injection::check_output(user_id, reply, &cfg.anti_injection);
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

    send_msg(group_id, user_id, reply);
    true
}

