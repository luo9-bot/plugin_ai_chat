//! 发言守门：识别绝不该从她嘴里说出来的话
//!
//! "文本即发言"的管线下，模型偶尔会把本应走 `tool_calls` 机制的
//! 工具调用（`finish(reason: ...)` 之类），或记忆流的转写格式
//! （`[名字 15:16] "消息"`）当成文字输出。这类文本一旦进入发送
//! 管线就会原样发到群里。守门提供两道检测，命中者绝不外发：
//! - `detect_leaked_tool_call`：工具调用语法泄漏 → 沉默或重试
//! - `is_transcribed_echo`：转写格式复读 → 剔除该段

/// 检测纯文本是否为泄漏的工具调用语法，返回泄漏的工具名
///
/// 只做整条匹配，不误伤正常发言。覆盖四种形态：
/// - 裸工具名：`finish`
/// - 括号调用：`finish(reason: ...)`（中英文括号/引号混排也算）
/// - 冒号形态：`finish: 不想说`
/// - JSON 形态：`{"name": "finish", "arguments": "..."}`
pub fn detect_leaked_tool_call<'n>(text: &str, tool_names: &[&'n str]) -> Option<&'n str> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    // JSON 形态：整条是对象、带 "name" 键且点名了某个已知工具
    if text.starts_with('{')
        && text.ends_with('}')
        && text.contains("\"name\"")
        && let Some(found) = tool_names.iter().find(|n| text.contains(**n))
    {
        return Some(found);
    }

    for name in tool_names {
        let Some(rest) = text.strip_prefix(name) else {
            continue;
        };
        // 裸工具名：整条就是工具名
        if rest.is_empty() {
            return Some(name);
        }
        let after = rest.trim_start();
        // 括号形态：tool( ... ) / tool（ ... ），参数内容不限制
        let opens_bracket = after.starts_with('(') || after.starts_with('（');
        let closes_bracket = text.ends_with(')') || text.ends_with('）');
        if opens_bracket && closes_bracket {
            return Some(name);
        }
        // 冒号形态：tool: xxx / tool：xxx
        if after.starts_with(':') || after.starts_with('：') {
            return Some(name);
        }
    }
    None
}

/// 检测一段文字是否为记忆流转写格式的复读
///
/// 转写格式 `[名字 15:16] "消息"` 是系统给她看的记录形态，
/// 不是她说话的方式。允许 `名字：` 前缀；整段匹配才算命中，
/// 夹着评论的混合内容不误伤。
pub fn is_transcribed_echo(text: &str) -> bool {
    let text = text.trim();
    // 带名字前缀时先剥掉（名字里不含冒号，剥首个冒号前的部分）
    let body = if text.starts_with('[') {
        text
    } else {
        match text.split_once('：').or_else(|| text.split_once(':')) {
            Some((_, rest)) if rest.trim().starts_with('[') => rest.trim(),
            _ => return false,
        }
    };

    let Some(after_open) = body.strip_prefix('[') else {
        return false;
    };
    let Some(close_pos) = after_open.find(']') else {
        return false;
    };
    let label = &after_open[..close_pos];

    // 标签形如 "名字 HH:MM"：最后一个空格后是时间
    let Some((_, time)) = label.rsplit_once(' ') else {
        return false;
    };
    if !is_hh_mm(time) {
        return false;
    }

    // 时间后紧跟成对引号包裹的内容
    let quoted = after_open[close_pos + 1..].trim_start();
    [("“", "”"), ("\"", "\""), ("「", "」")]
        .iter()
        .any(|(open, close)| {
            quoted.starts_with(open)
                && quoted.ends_with(close)
                && quoted.chars().count() > open.chars().count() + close.chars().count()
        })
}

/// 判断是否为 `HH:MM` / `H:MM` 形态的合法时间
fn is_hh_mm(time: &str) -> bool {
    let Some((h, m)) = time.split_once(':') else {
        return false;
    };
    let (Ok(hour), Ok(minute)) = (h.parse::<u32>(), m.parse::<u32>()) else {
        return false;
    };
    (1..=2).contains(&h.len()) && m.len() == 2 && hour < 24 && minute < 60
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOOLS: &[&str] = &["query_memory", "send_sticker", "finish", "plan_next"];

    #[test]
    fn bare_tool_name_is_leak() {
        assert_eq!(detect_leaked_tool_call("finish", TOOLS), Some("finish"));
        assert_eq!(detect_leaked_tool_call("  finish  ", TOOLS), Some("finish"));
    }

    #[test]
    fn bracketed_call_is_leak() {
        assert_eq!(
            detect_leaked_tool_call("finish(reason: 不想说)", TOOLS),
            Some("finish")
        );
        // 中英文括号混排（真实日志案例）
        assert_eq!(
            detect_leaked_tool_call("finish(reason: 群里就是在玩表情和梗，我没啥要接的）", TOOLS),
            Some("finish")
        );
        assert_eq!(
            detect_leaked_tool_call("send_sticker()", TOOLS),
            Some("send_sticker")
        );
        assert_eq!(
            detect_leaked_tool_call("plan_next(in_secs: 300, reason: 一会儿再想)", TOOLS),
            Some("plan_next")
        );
    }

    #[test]
    fn colon_form_is_leak() {
        assert_eq!(
            detect_leaked_tool_call("finish: 不想说", TOOLS),
            Some("finish")
        );
        assert_eq!(
            detect_leaked_tool_call("finish：不想说", TOOLS),
            Some("finish")
        );
    }

    #[test]
    fn json_form_is_leak() {
        assert_eq!(
            detect_leaked_tool_call(
                "{\"name\": \"finish\", \"arguments\": \"{\\\"reason\\\": \\\"没什么\\\"}\"}",
                TOOLS
            ),
            Some("finish")
        );
    }

    #[test]
    fn normal_speech_passes() {
        assert_eq!(detect_leaked_tool_call("这你也学", TOOLS), None);
        assert_eq!(detect_leaked_tool_call("我 finish 了作业", TOOLS), None);
        // 前缀词不误伤：finisher 不是 finish 调用
        assert_eq!(detect_leaked_tool_call("finisher!", TOOLS), None);
        assert_eq!(detect_leaked_tool_call("说好了一起 finish", TOOLS), None);
        // 括号没闭合不算调用
        assert_eq!(
            detect_leaked_tool_call("finish(reason 还没想好", TOOLS),
            None
        );
    }

    #[test]
    fn transcribed_echo_detected() {
        assert!(is_transcribed_echo("土豆：[土豆 15:16] “finish”"));
        assert!(is_transcribed_echo("[土豆 15:16] “finish”"));
        assert!(is_transcribed_echo("土豆: [土豆 15:16] \"finish\""));
        assert!(is_transcribed_echo("[洛玖 3:05] 「晚上吃火锅」"));
        assert!(is_transcribed_echo("  [土豆 15:16] “finish”  "));
    }

    #[test]
    fn normal_quoted_text_not_echo() {
        // 不以转写格式开头
        assert!(!is_transcribed_echo("他发了条[土豆 15:16] “finish”给我"));
        // 夹着评论、结尾不是闭引号
        assert!(!is_transcribed_echo("[土豆 15:16] “finish” 你发这个干嘛"));
        // 普通发言
        assert!(!is_transcribed_echo("这你也学"));
        // 标签里没有时间
        assert!(!is_transcribed_echo("[土豆] “finish”"));
        // 时间格式不对
        assert!(!is_transcribed_echo("[土豆 99:88] “finish”"));
    }
}
