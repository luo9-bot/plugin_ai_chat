//! 消息分段处理：规范化分隔符、分割消息

/// 消息分段分隔符
pub const SEGMENT_SEP: &str = "|^|";

/// 单次发送最多拆分的消息条数：真人连发一般不会超过 3~4 条
const MAX_SEGMENT_COUNT: usize = 4;

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

/// 清理 AI 回复：移除自记忆标签、Unicode emoji
///
/// 所有 AI 生成的消息发送前都应经过此函数。
/// 不改动她的文字本身——空格是自然的停顿，语气由她自己控制。
pub fn clean_reply(reply: &str) -> String {
    // 1. 移除自记忆分类标签 [经历] [反思] [计划] [感受]
    const SELF_TAGS: &[&str] = &["[经历]", "[反思]", "[计划]", "[感受]"];
    let mut result = reply.to_string();
    for tag in SELF_TAGS {
        result = result.replace(tag, "");
    }

    // 2. 移除 Unicode emoji（AI 不应发送 emoji，使用表情包系统代替）
    result = crate::emoji::strip_emoji(&result);

    // 3. 规范化连续分段符
    result.replace("|^||^|", "|^|")
}

/// 将已规范化的消息按 `|^|` 和换行分割为最终发送片段
pub fn split_segments(normalized_reply: &str) -> Vec<String> {
    let mut segments: Vec<String> = if normalized_reply.contains(SEGMENT_SEP) {
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
    };

    // 限制分段数量：超出部分并入最后一段（保留换行，避免不同段落粘连）
    if segments.len() > MAX_SEGMENT_COUNT {
        let overflow: String = segments.split_off(MAX_SEGMENT_COUNT).join("\n");
        if let Some(last) = segments.last_mut() {
            last.push('\n');
            last.push_str(&overflow);
        }
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_incomplete_separators() {
        assert_eq!(normalize_segment_sep("嗯^|好"), "嗯|^|好");
        assert_eq!(normalize_segment_sep("嗯|^好"), "嗯|^|好");
        assert_eq!(normalize_segment_sep("嗯|^||^|好"), "嗯|^|好");
        assert_eq!(normalize_segment_sep("嗯\n\n好"), "嗯\n好");
        // 单个 ^ 不是分隔符
        assert_eq!(normalize_segment_sep("a^b"), "a^b");
    }

    #[test]
    fn splits_by_separator_and_newline() {
        assert_eq!(split_segments("嗯|^|好|^|再见"), vec!["嗯", "好", "再见"]);
        assert_eq!(split_segments("嗯\n好"), vec!["嗯", "好"]);
        assert_eq!(split_segments("嗯|^|好\n再见"), vec!["嗯", "好", "再见"]);
        // 空段被剔除
        assert_eq!(split_segments("嗯|^|\n|^|好"), vec!["嗯", "好"]);
    }

    #[test]
    fn overflow_segments_join_with_newline() {
        let joined = split_segments("一|^|二|^|三|^|四|^|五|^|六");
        assert_eq!(joined.len(), MAX_SEGMENT_COUNT);
        assert_eq!(joined[0], "一");
        assert_eq!(joined[3], "四\n五\n六");
    }

    #[test]
    fn within_limit_stays_unmerged() {
        assert_eq!(split_segments("一|^|二|^|三|^|四").len(), 4);
    }
}
