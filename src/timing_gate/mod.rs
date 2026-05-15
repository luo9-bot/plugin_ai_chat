//! Timing Gate 决策模块
//!
//! 替代原有的 batch_decide，用社交语境判断替代硬性规则。


use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info, warn};

/// Timing Gate 决策结果
#[derive(Debug, Clone, PartialEq)]
pub enum GateDecision {
    Continue,
    NoReply,
    Wait(u64),
}

/// Timing Gate 上下文
pub struct GateContext {
    pub identity: String,
    pub recent_bot_messages: Vec<String>,
    pub working_memory: String,
    pub self_qq: u64,
    pub is_group: bool,
}

/// NoReply 冷却记录 (group_id -> last_no_reply_timestamp)
static COOLDOWN_STATE: Mutex<Option<HashMap<u64, u64>>> = Mutex::new(None);

/// 冷却时间（秒）
const NO_REPLY_COOLDOWN_SECS: u64 = 120;
/// 最大重试次数
const MAX_ATTEMPTS: u32 = 3;
/// 上下文保留比例（只保留最近 30% 的消息）
const CONTEXT_KEEP_RATIO: f64 = 0.3;

// ── 工具定义 ────────────────────────────────────────────────────

fn tool_continue() -> crate::ai::Tool {
    crate::ai::Tool {
        tool_type: "function".into(),
        function: crate::ai::FunctionDef {
            name: "continue".into(),
            description: "允许当前会话继续到下一轮思考。当你判断 bot 需要真正回复或收集信息时调用。".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "reason": { "type": "string", "description": "为什么选择继续" }
                },
                "required": ["reason"]
            }),
        },
    }
}

fn tool_no_reply() -> crate::ai::Tool {
    crate::ai::Tool {
        tool_type: "function".into(),
        function: crate::ai::FunctionDef {
            name: "no_reply".into(),
            description: "停止思考，不回复，等待新消息。当你判断 bot 不应该插嘴时调用。".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "reason": { "type": "string", "description": "为什么选择沉默" }
                },
                "required": ["reason"]
            }),
        },
    }
}

fn tool_wait() -> crate::ai::Tool {
    crate::ai::Tool {
        tool_type: "function".into(),
        function: crate::ai::FunctionDef {
            name: "wait".into(),
            description: "暂停等待一段时间，然后重新评估。用于用户可能还有话要说的情况。".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "seconds": { "type": "integer", "description": "等待秒数（建议 10-60）" },
                    "reason": { "type": "string", "description": "为什么选择等待" }
                },
                "required": ["seconds", "reason"]
            }),
        },
    }
}

// ── 冷却管理 ────────────────────────────────────────────────────

/// 检查是否在冷却期内
pub fn is_in_cooldown(group_id: u64) -> bool {
    let guard = COOLDOWN_STATE.lock().unwrap();
    if let Some(ref map) = *guard
        && let Some(&last) = map.get(&group_id) {
            return crate::util::now_secs().saturating_sub(last) < NO_REPLY_COOLDOWN_SECS;
        }
    false
}

/// 记录 NoReply 决策时间
fn record_no_reply(group_id: u64) {
    let mut guard = COOLDOWN_STATE.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    map.insert(group_id, crate::util::now_secs());
}

// ── 主逻辑 ──────────────────────────────────────────────────────

pub fn run_timing_gate(
    group_id: u64,
    messages: &[&(u64, String, Vec<u64>)],
    context: &GateContext,
) -> GateDecision {
    let cfg = crate::config::get();
    let bot_name = &cfg.bot_name;

    // 使用 luo9_timing_gate.prompt 作为主模板，渲染 bot_name + identity
    let mut prompt_vars: HashMap<&str, &str> = HashMap::new();
    prompt_vars.insert("bot_name", bot_name);
    prompt_vars.insert("identity", &context.identity);
    prompt_vars.insert("timing_gate_wait_rule", "");
    prompt_vars.insert("group_chat_attention_block", "");
    let prompt = crate::prompt::PromptManager::get().render("luo9_timing_gate", &prompt_vars);

    // 截断上下文：只保留最近的消息（降低 token 消耗）
    let keep = ((messages.len() as f64 * CONTEXT_KEEP_RATIO) as usize).max(3);
    let truncated: Vec<_> = if messages.len() > keep {
        messages[messages.len() - keep..].to_vec()
    } else {
        messages.to_vec()
    };

    // 构建消息流描述
    let batch_lines: Vec<String> = truncated
        .iter()
        .map(|(uid, msg, _)| {
            let text = crate::vision::strip_image_cq(msg);
            let display = if text.is_empty() { "[图片]" } else { &text };
            format!("[user_id:{}] {}", uid, display)
        })
        .collect();

    // 构建上下文
    let mut context_parts = Vec::new();
    if !context.identity.is_empty() {
        context_parts.push(format!("# 你的身份\n{}", context.identity));
    }
    if context.self_qq > 0 {
        context_parts.push(format!(
            "# 你的 QQ 号\n{}\n有人 @[CQ:at,qq={}] 可能代表有人和你说话",
            context.self_qq, context.self_qq
        ));
    }
    if !context.recent_bot_messages.is_empty() {
        context_parts.push(format!(
            "# 你在群里最近的消息\n{}",
            context.recent_bot_messages.join("\n")
        ));
    }
    if !context.working_memory.is_empty() {
        context_parts.push(context.working_memory.clone());
    }

    let full_prompt = format!("{}\n\n{}", prompt, context_parts.join("\n\n"));
    let content = format!(
        "群聊消息流（分析聊天节奏，决定是否回复）:\n{}",
        batch_lines.join("\n")
    );

    // 工具列表：群聊只有 continue/no_reply，私聊额外支持 wait
    let mut tools = vec![tool_continue(), tool_no_reply()];
    if !context.is_group {
        tools.push(tool_wait());
    }

    // 重试循环
    debug!(group_id, "timing_gate: evaluating");
    for attempt in 0..MAX_ATTEMPTS {
        match crate::ai::analyze_with_tools_named(
            &full_prompt,
            &content,
            &tools,
            Some(serde_json::json!("auto")),
        ) {
            Ok((name, args)) => {
                let reason = args.get("reason").and_then(|v| v.as_str()).unwrap_or("");
                match name.as_str() {
                    "continue" => {
                        info!(group_id, reason, "timing_gate: continue");
                        return GateDecision::Continue;
                    }
                    "no_reply" => {
                        debug!(group_id, reason, "timing_gate: no_reply");
                        record_no_reply(group_id);
                        return GateDecision::NoReply;
                    }
                    "wait" => {
                        let seconds = args.get("seconds").and_then(|v| v.as_u64()).unwrap_or(30);
                        info!(group_id, seconds, reason, "timing_gate: wait");
                        return GateDecision::Wait(seconds);
                    }
                    other => {
                        warn!(group_id, name = %other, attempt, "timing_gate: invalid tool, retrying");
                        // 注入提示重试
                        if attempt < MAX_ATTEMPTS - 1 {
                            continue;
                        }
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, group_id, attempt, "timing_gate: AI error, retrying");
                if attempt < MAX_ATTEMPTS - 1 {
                    continue;
                }
            }
        }
    }

    // 所有重试失败，默认沉默
    warn!(group_id, "timing_gate: all attempts failed, defaulting to NoReply");
    record_no_reply(group_id);
    GateDecision::NoReply
}

// ── 辅助函数 ────────────────────────────────────────────────────

pub fn has_at_bot(messages: &[&(u64, String, Vec<u64>)], self_qq: u64) -> bool {
    if self_qq == 0 { return false; }
    let at_pattern = format!("[CQ:at,qq={}]", self_qq);
    messages.iter().any(|(_, msg, _)| msg.contains(&at_pattern))
}

pub fn mentions_bot(messages: &[&(u64, String, Vec<u64>)], bot_name: &str) -> bool {
    if bot_name.is_empty() { return false; }
    messages.iter().any(|(_, msg, _)| msg.contains(bot_name))
}
