//! 消息分段处理：规范化分隔符、分割消息

/// 消息分段分隔符
pub const SEGMENT_SEP: &str = "|^|";

/// 规范化消息分段分隔符：将 AI 生成的不完整 "|^" 或 "^|" 补全为 "|^|"
pub fn normalize_segment_sep(reply: &str) -> String {
    let mut normalized = reply.replace("\r\n", "\n");
    while normalized.contains("\n\n") {
        normalized = normalized.replace("\n\n", "\n");
    }

    if !normalized.contains('^') {
        return normalized;
    }

    let chars: Vec<char> = normalized.chars().collect();
    let len = chars.len();
    let mut is_sep: Vec<bool> = vec![false; len];

    for i in 0..len {
        if chars[i] != '^' {
            continue;
        }
        let has_pipe_before = i > 0 && chars[i - 1] == '|';
        let has_pipe_after = i + 1 < len && chars[i + 1] == '|';
        if has_pipe_before || has_pipe_after {
            is_sep[i] = true;
            if has_pipe_before {
                is_sep[i - 1] = true;
            }
            if has_pipe_after {
                is_sep[i + 1] = true;
            }
        }
    }

    let mut out = String::with_capacity(len + 8);
    let mut i = 0;
    while i < len {
        if is_sep[i] {
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
        normalized_reply
            .split(SEGMENT_SEP)
            .flat_map(|s| s.split('\n'))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        normalized_reply
            .split('\n')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}
