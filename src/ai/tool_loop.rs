//! 表达管线的多轮工具调用循环
//!
//! 与 `analyze_with_tools_named` 的本质区别：
//! - 纯文本响应直接作为输出返回（"文本即发言"），不强制包装成工具调用
//! - 接受完整的对话历史与多轮工具结果累积
//! - 空响应 = 她选择沉默，是合法结果而非错误
//!
//! 工具结果的回传沿用既有模式：以文本形式追加到消息列表，
//! 避免依赖各后端对 `tool` 角色 / tool_call_id 的兼容性差异。

use tracing::debug;

use super::provider::{no_error_agent, track_usage};
use super::types::{ChatMessage, ChatRequest, ChatResponse, Tool, ToolOutcome};
use crate::config;

/// 运行表达工具循环
///
/// - `history`: 已有的对话轮次 (role, content)，role 为 "user"/"assistant"
/// - `user_content`: 本轮用户内容（当前消息或群聊场景记录）
/// - `on_tool`: 工具执行回调，返回 `Continue(结果)` 继续推理，`Abort` 立即沉默
///
/// 返回 `Ok(Some(text))` = 她要说的话；`Ok(None)` = 沉默；`Err` = API 失败。
pub fn run_tool_loop(
    system_prompt: &str,
    history: &[(String, String)],
    user_content: &str,
    tools: &[Tool],
    max_rounds: u32,
    mut on_tool: impl FnMut(&str, &serde_json::Value) -> ToolOutcome,
) -> Result<Option<String>, String> {
    let cfg = config::get();
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let agent = no_error_agent();

    let mut messages: Vec<ChatMessage> = Vec::with_capacity(history.len() + 2);
    messages.push(ChatMessage {
        role: "system".to_string(),
        content: Some(system_prompt.to_string()),
        tool_calls: None,
        reasoning_content: None,
    });
    for (role, content) in history {
        messages.push(ChatMessage {
            role: role.clone(),
            content: Some(content.clone()),
            tool_calls: None,
            reasoning_content: None,
        });
    }
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: Some(user_content.to_string()),
        tool_calls: None,
        reasoning_content: None,
    });

    for round in 0..max_rounds {
        let req = ChatRequest {
            model: cfg.model.clone(),
            messages: messages.clone(),
            frequency_penalty: cfg.ai.frequency_penalty,
            presence_penalty: cfg.ai.presence_penalty,
            temperature: cfg.ai.temperature,
            top_p: cfg.ai.top_p,
            max_tokens: cfg.ai.max_tokens,
            tools: Some(tools.to_vec()),
            tool_choice: Some(serde_json::json!("auto")),
            thinking: Some(serde_json::json!({"type": "disabled"})),
        };

        let json_body =
            serde_json::to_string(&req).map_err(|e| format!("Serialize failed: {e}"))?;
        let mut resp = agent
            .post(&url)
            .header("Authorization", &format!("Bearer {}", cfg.api_key))
            .header("Content-Type", "application/json")
            .send(json_body.as_bytes())
            .map_err(|e| format!("API request failed: {e}"))?;

        let status = resp.status();
        let resp_str = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("API read failed: {e}"))?;
        if !(200..300).contains(&status.as_u16()) {
            return Err(format!("API returned {}: {resp_str}", status.as_u16()));
        }

        let body: ChatResponse =
            serde_json::from_str(&resp_str).map_err(|e| format!("API parse failed: {e}"))?;
        track_usage(&body, "voice", &cfg.model);

        let choice = body
            .choices
            .into_iter()
            .next()
            .ok_or("API returned empty choices")?;
        let message = choice.message;

        // 工具调用：执行并回传结果，继续下一轮
        if let Some(tool_calls) = &message.tool_calls
            && let Some(first) = tool_calls.first()
        {
            let name = first.function.name.clone();
            let args = serde_json::from_str::<serde_json::Value>(&first.function.arguments)
                .unwrap_or(serde_json::json!({}));
            debug!(round, tool = %name, "tool_loop: tool call");
            match on_tool(&name, &args) {
                ToolOutcome::Abort => return Ok(None),
                ToolOutcome::Continue(result) => {
                    messages.push(ChatMessage {
                        role: "user".to_string(),
                        content: Some(format!(
                            "（你刚才调用了 {name}）\n[执行结果]\n{result}\n\n请继续：直接输出你要说的话（输出即发言），或调用 finish 保持沉默。"
                        )),
                        tool_calls: None,
                        reasoning_content: None,
                    });
                }
            }
            continue;
        }

        // 纯文本响应 = 她要说的话
        let mut text = message.content.unwrap_or_default();
        if let Some(pos) = text.find("</think>") {
            text = text[pos + 8..].to_string();
        }
        let text = text.trim().to_string();
        if !text.is_empty() {
            return Ok(Some(text));
        }

        // 空响应 = 沉默
        debug!(round, "tool_loop: empty response, staying silent");
        return Ok(None);
    }

    debug!(max_rounds, "tool_loop: max rounds reached, staying silent");
    Ok(None)
}
