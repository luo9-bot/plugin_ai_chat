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

/// 清理 AI 回复：移除自记忆标签、Unicode emoji，将中文字符间的空格转为分段符
///
/// 所有 AI 生成的消息发送前都应经过此函数。
pub fn clean_reply(reply: &str) -> String {
    // 1. 移除自记忆分类标签 [经历] [反思] [计划] [感受]
    const SELF_TAGS: &[&str] = &["[经历]", "[反思]", "[计划]", "[感受]"];
    let mut result = reply.to_string();
    for tag in SELF_TAGS {
        result = result.replace(tag, "");
    }

    // 2. 移除 Unicode emoji（AI 不应发送 emoji，使用表情包系统代替）
    result = crate::emoji::strip_emoji(&result);

    // 3. 将中文字符之间的空格转为 |^| 分段符
    let chars: Vec<char> = result.chars().collect();
    let mut out = String::with_capacity(result.len());
    for i in 0..chars.len() {
        if chars[i] == ' ' && i > 0 && i + 1 < chars.len()
            && is_cjk(chars[i - 1]) && is_cjk(chars[i + 1])
        {
            out.push_str("|^|");
        } else {
            out.push(chars[i]);
        }
    }

    // 3. 规范化连续分段符
    out.replace("|^||^|", "|^|")
}

fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}'   |
        '\u{3400}'..='\u{4DBF}'   |
        '\u{F900}'..='\u{FAFF}'   |
        '\u{20000}'..='\u{2A6DF}' |
        '\u{2A700}'..='\u{2B73F}' |
        '\u{2B740}'..='\u{2B81F}' |
        '\u{3001}'..='\u{3003}'   |
        '\u{300C}'..='\u{3011}'   |
        '\u{FF01}'..='\u{FF5E}'   |
        '\u{2026}'
    )
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
