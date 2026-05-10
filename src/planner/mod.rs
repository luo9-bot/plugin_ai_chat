//! Planner 多轮推理引擎
//!
//! 替代原有的单次 AI 调用，支持多轮工具调用收集信息后再回复。
//! 参考 MaiBot 的 reasoning_engine 架构。

use tracing::{debug, info, warn};

/// Planner 推理结果
#[derive(Debug)]
pub enum PlannerAction {
    /// 需要回复某个用户
    Reply {
        user_id: u64,
        group_id: u64,
        message: String,
        reference_info: Option<String>,
    },
    /// 不回复（finish 工具）
    Silent,
}

/// Planner 上下文
pub struct PlannerContext {
    pub group_id: u64,
    pub user_id: u64,
    pub user_message: String,
    pub identity: String,
    pub extra_context: String,
    pub history: Vec<(String, String)>,
}

/// Planner 工具：reply
fn tool_reply() -> crate::ai::Tool {
    crate::ai::Tool {
        tool_type: "function".to_string(),
        function: crate::ai::FunctionDef {
            name: "reply".to_string(),
            description: "当你判断 bot 应该正式发送一条可见回复时调用。回复内容由回复生成器根据上下文自动生成。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "reference_info": {
                        "type": "string",
                        "description": "回复参考信息，告诉回复生成器应该基于什么来回复（如：用户问了什么、需要回答什么）"
                    }
                },
                "required": ["reference_info"]
            }),
        },
    }
}

/// Planner 工具：query_memory
fn tool_query_memory() -> crate::ai::Tool {
    crate::ai::Tool {
        tool_type: "function".to_string(),
        function: crate::ai::FunctionDef {
            name: "query_memory".to_string(),
            description: "查询关于某个用户的长期记忆。当回复需要了解用户的历史信息时使用。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "user_id": {
                        "type": "integer",
                        "description": "要查询的用户 QQ 号"
                    },
                    "query": {
                        "type": "string",
                        "description": "查询关键词"
                    }
                },
                "required": ["user_id"]
            }),
        },
    }
}

/// Planner 工具：query_person_info
fn tool_query_person_info() -> crate::ai::Tool {
    crate::ai::Tool {
        tool_type: "function".to_string(),
        function: crate::ai::FunctionDef {
            name: "query_person_info".to_string(),
            description: "查询用户的人物档案（名字、认识次数、首次见面时间等）。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "user_id": {
                        "type": "integer",
                        "description": "要查询的用户 QQ 号"
                    }
                },
                "required": ["user_id"]
            }),
        },
    }
}

/// Planner 工具：finish
fn tool_finish() -> crate::ai::Tool {
    crate::ai::Tool {
        tool_type: "function".to_string(),
        function: crate::ai::FunctionDef {
            name: "finish".to_string(),
            description: "结束本轮推理，不回复。当你判断不需要回复时调用。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "为什么选择不回复"
                    }
                },
                "required": ["reason"]
            }),
        },
    }
}

/// 执行 Planner 工具调用
fn execute_tool(name: &str, args: &serde_json::Value, _ctx: &PlannerContext) -> String {
    match name {
        "query_memory" => {
            let user_id = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
            if user_id == 0 {
                return "错误：未提供 user_id".to_string();
            }
            let memories = crate::memory::get_context(user_id);
            if memories.is_empty() {
                format!("用户 {} 没有已知的记忆", user_id)
            } else {
                memories
            }
        }
        "query_person_info" => {
            let user_id = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
            if user_id == 0 {
                return "错误：未提供 user_id".to_string();
            }
            // 从记忆中获取人物信息
            let memories = crate::memory::get_context(user_id);
            if memories.is_empty() {
                format!("用户 {} 没有已知的人物信息", user_id)
            } else {
                memories
            }
        }
        "reply" | "finish" => {
            // 这两个工具在主循环中处理
            String::new()
        }
        _ => {
            warn!(name, "planner: unknown tool");
            format!("未知工具：{}", name)
        }
    }
}

/// 运行 Planner 多轮推理
///
/// 返回 PlannerAction，指示是否需要回复。
pub fn run_planner(ctx: &PlannerContext) -> PlannerAction {
    let max_rounds = 5;
    let prompt = crate::prompt::PromptManager::get().raw("planner");

    let tools = vec![
        tool_reply(),
        tool_query_memory(),
        tool_query_person_info(),
        tool_finish(),
    ];

    // 构建系统 prompt
    let system_prompt = format!("{}\n\n{}", prompt, ctx.extra_context);

    // 构建用户消息（包含历史 + 当前消息）
    let mut user_content = String::new();
    if !ctx.history.is_empty() {
        user_content.push_str("# 对话历史\n");
        for (role, content) in &ctx.history {
            user_content.push_str(&format!("{}: {}\n", role, content));
        }
        user_content.push('\n');
    }
    user_content.push_str(&format!("# 当前消息\n[user_id:{}] {}", ctx.user_id, ctx.user_message));

    let mut gathered_info = Vec::new();

    for round in 0..max_rounds {
        debug!(round, group_id = ctx.group_id, user_id = ctx.user_id, "planner: round");

        match crate::ai::analyze_with_tools_named(
            &system_prompt,
            &user_content,
            &tools,
            Some(serde_json::json!("auto")),
        ) {
            Ok((name, args)) => {
                match name.as_str() {
                    "reply" => {
                        let reference_info = args
                            .get("reference_info")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        info!(
                            user_id = ctx.user_id,
                            group_id = ctx.group_id,
                            reference_info = %reference_info,
                            gathered_count = gathered_info.len(),
                            "planner: reply"
                        );
                        return PlannerAction::Reply {
                            user_id: ctx.user_id,
                            group_id: ctx.group_id,
                            message: ctx.user_message.clone(),
                            reference_info: if reference_info.is_empty() {
                                None
                            } else {
                                Some(reference_info)
                            },
                        };
                    }
                    "finish" => {
                        let reason = args.get("reason").and_then(|v| v.as_str()).unwrap_or("");
                        debug!(user_id = ctx.user_id, group_id = ctx.group_id, reason, "planner: finish");
                        return PlannerAction::Silent;
                    }
                    tool_name => {
                        // 执行工具并将结果添加到上下文
                        let result = execute_tool(tool_name, &args, ctx);
                        if !result.is_empty() {
                            gathered_info.push(format!("[{} 结果] {}", tool_name, result));
                            // 将工具结果添加到用户消息中，供下一轮使用
                            user_content.push_str(&format!(
                                "\n\n[工具结果: {}]\n{}",
                                tool_name, result
                            ));
                        }
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, round, "planner: AI error");
                // 出错时默认沉默
                return PlannerAction::Silent;
            }
        }
    }

    // 达到最大轮数，默认沉默
    warn!(max_rounds, user_id = ctx.user_id, "planner: max rounds reached, defaulting to silent");
    PlannerAction::Silent
}
