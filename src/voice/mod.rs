//! 语音合一：感知、决策、表达由同一次调用完成
//!
//! 她不是"分析脑 → 发言者"的两段式傀儡，而是像人一样：读完场面之后，
//! 在同一口气里决定开口、沉默或发表情包，说出来的话就是最终发言。
//!
//! - 群聊：以带名字和时间戳的场景记录呈现整个群，她自行判断谁在跟谁说话
//! - 私聊：以正常的对话轮次呈现，全神贯注的一对一
//! - 沉默是合法输出：空响应或 finish 工具都意味着"这轮不说话"

use std::collections::HashMap;

use tracing::{debug, info};

use crate::ai::{Tool, ToolOutcome, run_tool_loop};
use crate::config;
use crate::conversation::context::{VoiceScene, build_voice_context};

/// 语音调用的最终决策
#[derive(Debug)]
pub enum VoiceAction {
    /// 她要说的话（可能是多条，用 |^| 或换行分隔）
    Reply(String),
    /// 这一轮她选择沉默
    Silent,
}

/// 群聊里一条待处理的发言
pub struct GroupUtterance {
    pub user_id: u64,
    pub text: String,
}

/// 群聊场景记录的时间窗口与条数上限
const TRANSCRIPT_WINDOW_SECS: u64 = 3600;
const TRANSCRIPT_MAX_ENTRIES: usize = 30;

// ── 工具定义 ────────────────────────────────────────────────────

fn query_memory_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: crate::ai::FunctionDef {
            name: "query_memory".to_string(),
            description: "查询关于某个人的长期记忆。只在回复明显依赖过去的事时使用：之前聊过的内容、对方的喜好、共同的经历、曾经的约定。闲聊寒暄不需要查。"
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "user_id": {"type": "integer", "description": "要查询的用户 QQ 号"},
                    "query": {"type": "string", "description": "想查什么，用自然语言描述"}
                },
                "required": ["user_id", "query"]
            }),
        },
    }
}

fn send_sticker_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: crate::ai::FunctionDef {
            name: "send_sticker".to_string(),
            description: "发表情包。当语言不够到位、想用图回应、或氛围需要时使用。系统会自动挑一张合适的。调用后你可以继续说话，也可以不说话。"
                .to_string(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        },
    }
}

fn finish_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: crate::ai::FunctionDef {
            name: "finish".to_string(),
            description: "决定什么都不说。沉默是正常且常常正确的选择。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"reason": {"type": "string", "description": "为什么不说话"}},
                "required": ["reason"]
            }),
        },
    }
}

fn voice_tools() -> Vec<Tool> {
    vec![query_memory_tool(), send_sticker_tool(), finish_tool()]
}

// ── 工具执行 ────────────────────────────────────────────────────

fn execute_tool(
    name: &str,
    args: &serde_json::Value,
    group_id: u64,
    primary_user_id: u64,
    transcript_tail: &[String],
) -> ToolOutcome {
    match name {
        "query_memory" => {
            let uid = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            if uid == 0 || query.is_empty() {
                return ToolOutcome::Continue("参数不完整，需要 user_id 和 query。".into());
            }
            let results = crate::memory::search_memories(uid, group_id, query, 10);
            if results.is_empty() {
                ToolOutcome::Continue(format!(
                    "关于用户 {uid} 没有查到相关记忆（查询: {query}）。凭眼前所见的继续就好。"
                ))
            } else {
                let lines: Vec<String> =
                    results.iter().map(|r| format!("- {}", r.content)).collect();
                ToolOutcome::Continue(format!("查到的记忆：\n{}", lines.join("\n")))
            }
        }
        "send_sticker" => {
            let recent_hashes =
                crate::runtime::reply_dedup::get_recent_sticker_hashes(group_id, 300);
            match crate::sticker::send_sticker(
                group_id,
                primary_user_id,
                transcript_tail,
                &recent_hashes,
            ) {
                Ok(desc) => {
                    info!(group_id, user_id = primary_user_id, desc = %desc, "voice: sent sticker");
                    ToolOutcome::Continue(format!(
                        "表情包已发出（{desc}）。你可以继续补一句话，或者就此打住。"
                    ))
                }
                Err(e) => {
                    ToolOutcome::Continue(format!("表情包没发出去（{e}）。想表达的话用文字说。"))
                }
            }
        }
        "finish" => {
            let reason = args.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            debug!(
                group_id,
                user_id = primary_user_id,
                reason,
                "voice: finish (silence)"
            );
            ToolOutcome::Abort
        }
        _ => {
            ToolOutcome::Continue("未知工具，可用工具：query_memory、send_sticker、finish。".into())
        }
    }
}

// ── 系统提示组装 ────────────────────────────────────────────────

fn build_system_prompt(scene: &VoiceScene, identity: &str) -> String {
    let cfg = config::get();
    let bot_name = &cfg.bot_name;

    let mut vars: HashMap<&str, &str> = HashMap::new();
    vars.insert("bot_name", bot_name);
    let frame = crate::prompt::PromptManager::get().render("voice", &vars);

    let mut parts = vec![frame, identity.to_string()];
    let rules = crate::prompt::PromptManager::get().raw("core_rules");
    if !rules.is_empty() {
        parts.push(rules.to_string());
    }

    let context = build_voice_context(scene);
    if !context.is_empty() {
        parts.push(context);
    }

    parts.push(format!("当前时间：{}", crate::util::now_formatted_cst()));
    parts.join("\n\n")
}

// ── 群聊 ────────────────────────────────────────────────────────

fn display_name(user_id: u64, group_id: u64) -> String {
    crate::person_info::get_display_name(user_id, group_id).unwrap_or_else(|| "群友".to_string())
}

/// 群聊场景记录：最近的消息流（旧在上）+ 本轮新消息（明确标出）
fn build_group_transcript(group_id: u64, utterances: &[GroupUtterance]) -> (String, Vec<String>) {
    let mut lines: Vec<String> = Vec::new();
    for entry in
        crate::working_memory::get_recent(group_id, TRANSCRIPT_WINDOW_SECS, TRANSCRIPT_MAX_ENTRIES)
    {
        let name = display_name(entry.user_id, group_id);
        lines.push(format!(
            "[{} {}] {}",
            name,
            crate::util::hh_mm(entry.timestamp),
            entry.content
        ));
    }

    let new_lines: Vec<String> = utterances
        .iter()
        .map(|u| format!("[{}] {}", display_name(u.user_id, group_id), u.text))
        .collect();

    let mut content = String::new();
    if !lines.is_empty() {
        content.push_str("# 群里的消息（最近一段时间的记录，旧在上）\n");
        content.push_str(&lines.join("\n"));
        content.push_str("\n\n");
    }
    content.push_str("# 刚刚到达的新消息（还没有人回应过）\n");
    content.push_str(&new_lines.join("\n"));

    (content, lines)
}

/// 群聊语音：她读完整个群的场面，决定说什么、对谁说，或者不说
pub fn speak_group(group_id: u64, utterances: &[GroupUtterance], force_reply: bool) -> VoiceAction {
    let mut involved: Vec<u64> = Vec::new();
    for u in utterances {
        if !involved.contains(&u.user_id) {
            involved.push(u.user_id);
        }
    }
    let primary = involved.first().copied().unwrap_or(0);
    let query_text = utterances
        .iter()
        .map(|u| u.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let scene = VoiceScene {
        group_id,
        primary_user_id: primary,
        involved_users: &involved,
        query_text: &query_text,
        force_reply,
    };

    let cfg = config::get();
    let identity = config::prompt();
    let system = build_system_prompt(&scene, &identity);
    let (user_content, transcript_lines) = build_group_transcript(group_id, utterances);

    debug!(group_id, users = ?involved, force_reply, "voice: group thinking");

    let transcript_tail: Vec<String> = transcript_lines
        .iter()
        .rev()
        .take(5)
        .rev()
        .cloned()
        .collect();

    let result = run_tool_loop(
        &system,
        &[],
        &user_content,
        &voice_tools(),
        cfg.conversation.voice_max_rounds,
        |name, args| execute_tool(name, args, group_id, primary, &transcript_tail),
    );

    match result {
        Ok(Some(text)) => VoiceAction::Reply(clean_voice_reply(&text, &cfg.bot_name)),
        Ok(None) => VoiceAction::Silent,
        Err(e) => {
            info!(group_id, error = %e, "voice: group API error");
            VoiceAction::Silent
        }
    }
}

// ── 私聊 ────────────────────────────────────────────────────────

/// 私聊语音：一对一，她全神贯注
///
/// `history` 是这段关系的对话轮次；`extra_system` 允许调用方追加
/// 特殊场景指令（如对话结束检测提示）。
pub fn speak_private(
    user_id: u64,
    message: &str,
    history: &[(String, String)],
    extra_system: Option<&str>,
) -> VoiceAction {
    let cfg = config::get();
    let involved = [user_id];
    let scene = VoiceScene {
        group_id: 0,
        primary_user_id: user_id,
        involved_users: &involved,
        query_text: message,
        force_reply: false,
    };

    let identity = config::prompt();
    let mut system = build_system_prompt(&scene, &identity);
    if let Some(extra) = extra_system {
        system.push_str("\n\n");
        system.push_str(extra);
    }

    debug!(user_id, "voice: private thinking");

    let transcript_tail: Vec<String> = history
        .iter()
        .rev()
        .take(5)
        .map(|(_, c)| c.clone())
        .collect();

    let result = run_tool_loop(
        &system,
        history,
        message,
        &voice_tools(),
        cfg.conversation.voice_max_rounds,
        |name, args| execute_tool(name, args, 0, user_id, &transcript_tail),
    );

    match result {
        Ok(Some(text)) => VoiceAction::Reply(clean_voice_reply(&text, &cfg.bot_name)),
        Ok(None) => VoiceAction::Silent,
        Err(e) => {
            info!(user_id, error = %e, "voice: private API error");
            VoiceAction::Silent
        }
    }
}

// ── 回复清理 ────────────────────────────────────────────────────

/// 最小化清理：去掉包裹引号和"名字："前缀
///
/// 不做任何"人性手术"——她的语气由她自己控制，
/// 分段、标点、表情都保持原样交给发送端按 |^| 和换行分泡。
fn clean_voice_reply(reply: &str, bot_name: &str) -> String {
    let mut text = reply.trim().to_string();

    // 去掉成对包裹的引号（模型偶尔会把整条发言包进引号）
    for (open, close) in [("“", "”"), ("\"", "\""), ("「", "」")] {
        if text.starts_with(open) && text.ends_with(close) && text.chars().count() > 2 {
            text = text[open.len()..text.len() - close.len()]
                .trim()
                .to_string();
        }
    }

    // 去掉 "洛玖：" / "洛玖:" 式的自我前缀
    let prefix = format!("{bot_name}：");
    if let Some(rest) = text.strip_prefix(&prefix) {
        text = rest.trim().to_string();
    }
    let prefix = format!("{bot_name}:");
    if let Some(rest) = text.strip_prefix(&prefix) {
        text = rest.trim().to_string();
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_wrapping_quotes() {
        assert_eq!(clean_voice_reply("“你好呀”", "洛玖"), "你好呀");
        assert_eq!(clean_voice_reply("\"嗯\"", "洛玖"), "嗯");
    }

    #[test]
    fn strips_name_prefix() {
        assert_eq!(clean_voice_reply("洛玖：来啦", "洛玖"), "来啦");
        assert_eq!(clean_voice_reply("来啦", "洛玖"), "来啦");
    }

    #[test]
    fn keeps_inner_quotes() {
        assert_eq!(
            clean_voice_reply("他说“好”就“好”吧", "洛玖"),
            "他说“好”就“好”吧"
        );
    }
}
