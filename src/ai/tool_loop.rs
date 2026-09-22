//! 表达管线的多轮工具调用循环
//!
//! - 纯文本响应直接作为输出返回（"文本即发言"），不强制包装成工具调用
//! - 接受完整的对话历史与多轮工具结果累积
//! - 空响应 = 她选择沉默，是合法结果而非错误
//!
//! **四种沉默在类型上可区分**（见 [`SilenceCause`]）：她选择不说、输出无效、
//! 上游故障、轮次用尽。它们曾经全都表现为 `Ok(None)`，于是"上游全挂了"和
//! "她今天很安静"在日志里一模一样。
//!
//! 工具结果的回传沿用既有模式：以文本形式追加到消息列表，
//! 避免依赖各后端对 `tool` 角色 / tool_call_id 的兼容性差异。

use tracing::debug;

use super::client::chat_completion;
use super::error::{LlmError, SilenceCause, TurnShape, Utterance};
use super::guard::{ALL_TOOL_NAMES, detect_leaked_tool_call};
use super::provider::track_usage;
use super::types::{ChatMessage, ChatRequest, Tool, ToolOutcome};
use crate::config;

/// 输出被判为无效（把工具调用写成文字）时，最多回传纠正几次
const MAX_INVALID_OUTPUT_CORRECTIONS: u32 = 2;

/// 记录一次影子观测（失败只留痕，绝不影响表达路径）
///
/// 它的唯一目的是回答"如果强制 Turn tagged union，现在有多少轮会变成
/// schema 失败"。观测在**每个出口**都要落一条，否则分布是有偏的。
fn observe(shape: TurnShape, detail: &str) {
    if let Err(error) = crate::db::db().record_turn_shadow(
        shape.as_str(),
        shape.is_valid_under_turn_contract(),
        detail,
    ) {
        tracing::debug!(%error, shape = shape.as_str(), "turn_shadow: 记录失败");
    }
}

/// 运行表达工具循环
///
/// - `history`: 已有的对话轮次 (role, content)，role 为 "user"/"assistant"
/// - `user_content`: 本轮用户内容（当前消息或群聊场景记录）
/// - `on_tool`: 工具执行回调，返回 `Continue(结果)` 继续推理，`Abort` 立即沉默
///
/// 返回 [`Utterance`]：要么是她要说的话，要么是一次**带原因**的沉默。
/// 失败原因放在值里而不是 `Err`，调用方无法"忽略错误"而丢掉它。
pub(crate) fn run_tool_loop(
    system_prompt: &str,
    history: &[(String, String)],
    user_content: &str,
    tools: &[Tool],
    max_rounds: u32,
    mut on_tool: impl FnMut(&str, &serde_json::Value) -> ToolOutcome,
) -> Utterance {
    let cfg = config::get();

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

    // 泄漏守门用全局工具清单：模型可能把本次没提供给它的工具名
    // （回神阶段的 say/catch_up、睡前整理的 write_diary…）写成文字
    let tool_names: &[&str] = ALL_TOOL_NAMES;
    let mut invalid_outputs = 0u32;

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

        let body = match chat_completion(&req) {
            Ok(body) => body,
            Err(error) => {
                // 上游故障：这不是她的沉默，必须留痕
                observe(TurnShape::UpstreamFailed, error.kind());
                return Utterance::failed(error);
            }
        };
        track_usage(&body, "voice", &cfg.model);

        let Some(choice) = body.choices.into_iter().next() else {
            observe(TurnShape::UpstreamFailed, "empty_choices");
            return Utterance::failed(LlmError::EmptyChoices);
        };
        let message = choice.message;

        // 工具调用：执行并回传结果，继续下一轮
        //
        // 一次响应只处理第一个工具调用，这是有意的：管线的语义是
        // "一个动作 + 一次发言"，`say`/`finish` 都是终止性动作，多调用
        // 并存本身就说明模型没想清楚。丢弃同批的其它调用，避免把
        // 自相矛盾的一轮（既 say 又 finish）当成合法决策。
        //
        // 同一响应里带 content 时同样丢弃：那是模型的"旁白"（真实案例
        // "然后呢?你要干嘛|^|say 内容"），不是发言。发言只能走 text 分支。
        if let Some(tool_calls) = &message.tool_calls
            && let Some(first) = tool_calls.first()
        {
            let name = first.function.name.clone();
            let has_narration = message
                .content
                .as_ref()
                .is_some_and(|c| !c.trim().is_empty());
            if has_narration {
                debug!(
                    round,
                    tool = %name,
                    "tool_loop: 丢弃与工具调用同时出现的旁白文本"
                );
            }
            if tool_calls.len() > 1 {
                debug!(
                    round,
                    count = tool_calls.len(),
                    "tool_loop: 一次响应含多个工具调用，只执行第一个"
                );
            }
            // 影子观测：严格 Turn 下这两种形态都会被 schema 拒绝
            if tool_calls.len() > 1 {
                observe(TurnShape::ToolCallMultiple, &name);
            } else if has_narration {
                observe(TurnShape::ToolCallWithNarration, &name);
            } else {
                observe(TurnShape::ToolCallClean, &name);
            }
            let args = serde_json::from_str::<serde_json::Value>(&first.function.arguments)
                .unwrap_or(serde_json::json!({}));
            debug!(round, tool = %name, "tool_loop: tool call");
            match on_tool(&name, &args) {
                // 工具主动要求收场（例如 say 已产出、或 finish）：这是她的决定
                ToolOutcome::Abort => return Utterance::silent(SilenceCause::Chose),
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

        // 纯文本响应 = 她要说的话（先过泄漏守门）
        let mut text = message.content.unwrap_or_default();
        if let Some(pos) = text.find("</think>") {
            text = text[pos + 8..].to_string();
        }
        let text = text.trim().to_string();
        if text.is_empty() {
            debug!(round, "tool_loop: empty response, staying silent");
            observe(TurnShape::EmptyResponse, "");
            return Utterance::silent(SilenceCause::Chose);
        }

        // 泄漏守门：模型把工具调用写成了文字（如 finish(reason: ...)）。
        // 这类文本绝不能当作发言外发：
        // - finish 形态 = 模型本意就是沉默，按沉默收场
        // - 其它形态 = 无效输出，回传纠正提示让她重新表达
        if let Some(leaked) = detect_leaked_tool_call(&text, tool_names) {
            if leaked == "finish" {
                debug!(round, "tool_loop: leaked finish text, treating as silence");
                observe(TurnShape::LeakedFinishText, leaked);
                return Utterance::silent(SilenceCause::Chose);
            }
            invalid_outputs += 1;
            observe(TurnShape::LeakedToolText, leaked);
            if invalid_outputs >= MAX_INVALID_OUTPUT_CORRECTIONS {
                debug!(
                    round,
                    attempts = invalid_outputs,
                    tool = leaked,
                    "tool_loop: 纠正后仍是泄漏的工具调用文本，判为无效输出"
                );
                return Utterance::silent(SilenceCause::InvalidOutput {
                    attempts: invalid_outputs,
                });
            }
            debug!(round, tool = leaked, "tool_loop: leaked tool call as text");
            messages.push(ChatMessage {
                role: "user".to_string(),
                content: Some(format!(
                    "（你把 {leaked} 的调用当成文字输出了——工具只能通过系统的工具调用机制使用，永远不能出现在发言文字里。）\n\n请继续：直接输出你要说的话（输出即发言），或通过工具调用机制调用 finish 保持沉默。"
                )),
                tool_calls: None,
                reasoning_content: None,
            });
            continue;
        }

        // 裸文本 = 发言。当前契约下这是常见且正常的路径，
        // 但在严格 Turn 下它会被判为无效输出（发言必须走 tool call）——
        // 这正是影子观测要量化的那一项。
        observe(TurnShape::PlainText, "");
        return Utterance::Say(text);
    }

    // 轮次用尽：若期间发生过无效输出，那才是更准确的病因
    observe(TurnShape::RoundsExhausted, "");
    if invalid_outputs > 0 {
        debug!(
            max_rounds,
            attempts = invalid_outputs,
            "tool_loop: 轮次用尽（期间存在无效输出）"
        );
        return Utterance::silent(SilenceCause::InvalidOutput {
            attempts: invalid_outputs,
        });
    }
    debug!(max_rounds, "tool_loop: max rounds reached, staying silent");
    Utterance::silent(SilenceCause::BudgetExhausted { rounds: max_rounds })
}
