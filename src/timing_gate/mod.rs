//! Timing Gate 决策模块
//!
//! 替代原有的 batch_decide，用社交语境判断替代硬性规则。
//! 参考 MaiBot 的两阶段决策架构：先判断"该不该说话"，再决定"说什么"。

use tracing::{debug, info};

/// Timing Gate 决策结果
#[derive(Debug, Clone, PartialEq)]
pub enum GateDecision {
    /// 继续到 Planner/回复生成阶段
    Continue,
    /// 沉默，等待新消息
    NoReply,
    /// 等待 N 秒后重新评估（仅私聊）
    Wait(u64),
}

/// Timing Gate 上下文
pub struct GateContext {
    /// bot 的人设 prompt
    pub identity: String,
    /// bot 最近在群里发的消息
    pub recent_bot_messages: Vec<String>,
    /// 群聊工作记忆
    pub working_memory: String,
    /// bot 的 QQ 号
    pub self_qq: u64,
}

/// Timing Gate 工具定义：continue
fn tool_continue() -> crate::ai::Tool {
    crate::ai::Tool {
        tool_type: "function".to_string(),
        function: crate::ai::FunctionDef {
            name: "continue".to_string(),
            description: "允许当前会话继续到下一轮思考。当你判断 bot 需要真正回复或收集信息时调用。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "为什么选择继续的原因"
                    }
                },
                "required": ["reason"]
            }),
        },
    }
}

/// Timing Gate 工具定义：no_reply
fn tool_no_reply() -> crate::ai::Tool {
    crate::ai::Tool {
        tool_type: "function".to_string(),
        function: crate::ai::FunctionDef {
            name: "no_reply".to_string(),
            description: "停止思考，不回复，等待新消息。当你判断 bot 不应该插嘴时调用。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "为什么选择沉默的原因"
                    }
                },
                "required": ["reason"]
            }),
        },
    }
}

/// 运行 Timing Gate 决策
///
/// 使用轻量级 AI 调用（仅 3 个工具），分析社交语境决定是否回复。
pub fn run_timing_gate(
    group_id: u64,
    messages: &[&(u64, String, Vec<u64>)],
    context: &GateContext,
) -> GateDecision {
    let prompt = crate::prompt::PromptManager::get().raw("timing_gate");

    // 构建消息流描述
    let batch_lines: Vec<String> = messages
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

    let tools = vec![tool_continue(), tool_no_reply()];

    debug!(group_id, "timing_gate: evaluating");
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
                    info!(group_id, reason, "timing_gate: continue -> 进入回复阶段");
                    GateDecision::Continue
                }
                "no_reply" => {
                    debug!(group_id, reason, "timing_gate: no_reply -> 沉默");
                    GateDecision::NoReply
                }
                "wait" => {
                    let seconds = args.get("seconds").and_then(|v| v.as_u64()).unwrap_or(30);
                    info!(group_id, seconds, reason, "timing_gate: wait");
                    GateDecision::Wait(seconds)
                }
                other => {
                    debug!(group_id, name = %other, "timing_gate: unknown tool, defaulting to NoReply");
                    GateDecision::NoReply
                }
            }
        }
        Err(e) => {
            debug!(error = %e, group_id, "timing_gate: AI error, defaulting to NoReply");
            GateDecision::NoReply
        }
    }
}

/// 检查是否有 @bot 消息（强制回复）
pub fn has_at_bot(messages: &[&(u64, String, Vec<u64>)], self_qq: u64) -> bool {
    if self_qq == 0 {
        return false;
    }
    let at_pattern = format!("[CQ:at,qq={}]", self_qq);
    messages.iter().any(|(_, msg, _)| msg.contains(&at_pattern))
}

/// 检查是否有提到 bot 名字的消息
pub fn mentions_bot(messages: &[&(u64, String, Vec<u64>)], bot_name: &str) -> bool {
    if bot_name.is_empty() {
        return false;
    }
    messages
        .iter()
        .any(|(_, msg, _)| msg.contains(bot_name))
}
