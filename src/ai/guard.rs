//! 发言守门：识别绝不该从她嘴里说出来的话
//!
//! "文本即发言"的管线下，模型偶尔会把本应走 `tool_calls` 机制的
//! 工具调用（`finish(reason: ...)` 之类），或记忆流的转写格式
//! （`[名字 15:16] "消息"`）当成文字输出。这类文本一旦进入发送
//! 管线就会原样发到群里。守门提供两道检测，命中者绝不外发：
//! - `detect_leaked_tool_call`：工具调用语法泄漏 → 整轮按沉默收场
//! - `is_transcribed_echo`：转写格式复读 → 剔除该段

/// 她见过的全部工具名（表达管线 + 回神决策 + 睡前整理）
///
/// 守门必须知道她能调用的每一个工具名，否则模型把没列进本次
/// 请求的工具名写成文字时就会漏网。这里是唯一权威清单：新增工具
/// 时同步补充，不必回查各处工具定义。
pub const ALL_TOOL_NAMES: &[&str] = &[
    // 表达 / 回神决策
    "say",
    "finish",
    "plan_next",
    "query_memory",
    "send_sticker",
    "search_web",
    "catch_up",
    // 睡前整理
    "write_diary",
    "update_person",
    "add_loop",
    "add_belief",
    "compress",
    "come_up_goal",
    "come_up_idea",
    "update_wish",
    // 其它会进她 prompt 的命名工具（分析/规划/审查）
    // 模型会把这些名字串味到发言里，日志里每天都能看到它们的踪迹
    "task_progress",
    "daily_plan",
    "weekly_plan",
    "monthly_plan",
    "memory_review",
    "decide_reply",
    "batch_decide",
    // 她自己的计划（表达与回神两条路径共用）
    "check_plan",
    "add_plan",
    "note_progress",
    "finish_plan",
];

/// 判断字符是否为中日韩文字或全角标点
///
/// 用于识别"工具名后面直接接中文正文"的粘连形态：正常发言里
/// 不会出现 `finish那个…`，但泄漏的内心独白几乎都长这样。
fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{3000}'..='\u{303f}'   // CJK 标点
        | '\u{4e00}'..='\u{9fff}' // CJK 统一表意
        | '\u{3400}'..='\u{4dbf}' // 扩展 A
        | '\u{f900}'..='\u{faff}' // 兼容表意
        | '\u{ff00}'..='\u{ffef}' // 全角形式
    )
}

/// 检测纯文本是否为泄漏的工具调用语法，返回泄漏的工具名
///
/// 只做整条匹配，不误伤正常发言。覆盖七种形态：
/// - 裸工具名：`finish`
/// - 括号调用：`finish(reason: ...)`（中英文括号/引号混排也算）
/// - 冒号形态：`finish: 不想说`
/// - JSON 形态：`{"name": "finish", "arguments": "..."}`
/// - 中文粘连：`finish那个@的号不是我…`（模型把工具名当前缀）
/// - 空格接中文：`finish 一条转发过来的消息…`
///
/// 后三种是真实日志里泄漏过的形态：模型一边想调用工具、一边把
/// 理由写成了正文。只要工具名出现在句首且紧跟正文，就认定整条
/// 不可信——宁可这轮沉默，也不能把内部语法发到群里。
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
        // 粘连形态：工具名后面直接跟中文（`finish那个…`）
        if rest.chars().next().is_some_and(is_cjk) {
            return Some(name);
        }
        // 空格接中文形态：`finish 一条转发过来的消息…`
        // 只在工具名后确实是空白（而非直接接英文单词）时成立，
        // 因此 `finisher!`、`finish the game` 这类不会误伤。
        if rest.starts_with(char::is_whitespace) && after.chars().next().is_some_and(is_cjk) {
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
///
/// 另一种形态是意识流渲染的 `[15:16] 内容`（无名字、无引号）——
/// 她的内心活动也是这样出现在"最近的经历"里的。复读它等于把
/// 内心独白当发言说出来，同样不该外发。
pub fn is_transcribed_echo(text: &str) -> bool {
    let text = text.trim();
    if let Some(rest) = leading_timestamp_echo(text) {
        return rest;
    }
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

/// 形如 `[15:16] 内容` 的复读判定
///
/// 时间是唯一的标签内容，且后面跟着实质文本。返回 Some(true/false)
/// 表示"确实是这种形态"，None 表示不是这种形态（交给其它判定）。
fn leading_timestamp_echo(text: &str) -> Option<bool> {
    let after_open = text.strip_prefix('[')?;
    let close_pos = after_open.find(']')?;
    let label = &after_open[..close_pos];
    if !is_hh_mm(label) {
        return None;
    }
    let body = after_open[close_pos + 1..].trim();
    // 空内容或极短内容不算复读（"[15:16]" 单独出现更可能是她在写时间）
    Some(body.chars().count() >= 2)
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

    const TOOLS: &[&str] = &["query_memory", "send_sticker", "finish", "plan_next", "say"];

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
        // 工具名在句中、后面接英文，不误伤
        assert_eq!(detect_leaked_tool_call("finish the game", TOOLS), None);
    }

    #[test]
    fn cjk_glued_tool_name_is_leak() {
        // 真实日志：2026-09-16 08:41:27
        // finish那个@的号不是我，神签的事跟我没关系，刚说过话就别急着插嘴了
        assert_eq!(
            detect_leaked_tool_call(
                "finish那个@的号不是我，神签的事跟我没关系，刚说过话就别急着插嘴了",
                TOOLS
            ),
            Some("finish")
        );
        // say 后面直接接中文
        assert_eq!(detect_leaked_tool_call("say内容", TOOLS), Some("say"));
    }

    #[test]
    fn space_then_cjk_tool_name_is_leak() {
        // 真实日志：2026-09-16 08:42:13
        // finish 一条转发过来的消息，没头没尾的，刚刚才插过一次话，先看着
        assert_eq!(
            detect_leaked_tool_call(
                "finish 一条转发过来的消息，没头没尾的，刚刚才插过一次话，先看着",
                TOOLS
            ),
            Some("finish")
        );
        // 全角空格同样算
        assert_eq!(
            detect_leaked_tool_call("say\u{3000}内容", TOOLS),
            Some("say")
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
    fn stream_render_echo_detected() {
        // 意识流渲染成 "[HH:MM] 内容"，没有名字也没有引号：
        // 复读它等于把内心独白当发言说出来
        assert!(is_transcribed_echo("[15:16] 她又在群里刷屏了"));
        assert!(is_transcribed_echo("[3:05] 想起：这话你今天已经说过了"));
        // 单独一个时间戳不算（她在写时间）
        assert!(!is_transcribed_echo("[15:16]"));
        // 不是时间的方括号内容不误伤
        assert!(!is_transcribed_echo("[图片1: 一只猫]"));
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
