//! Planner 多轮推理引擎

mod types;
mod tools;

pub use types::*;
use tools::*;

use tracing::{debug, info, warn};

fn execute_tool(name: &str, args: &serde_json::Value, ctx: &PlannerContext) -> String {
    match name {
        "query_memory" => {
            let uid = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
            if uid == 0 { return "错误：未提供 user_id".into(); }
            let m = crate::memory::get_context(uid);
            if m.is_empty() { format!("用户 {} 没有已知的记忆", uid) } else { m }
        }
        "query_person_info" => {
            let uid = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
            if uid == 0 { return "错误：未提供 user_id".into(); }
            let m = crate::memory::get_context(uid);
            if m.is_empty() { format!("用户 {} 没有已知的人物信息", uid) } else { m }
        }
        "send_emoji" => {
            let emotion = args.get("emotion").and_then(|v| v.as_str()).unwrap_or("开心");
            let context = format!("用户消息: {}", ctx.user_message);
            match crate::emoji_system::send_emoji(ctx.group_id, ctx.user_id, emotion, &context, &[]) {
                Ok(desc) => format!("已发送表情包: {}", desc),
                Err(e) => format!("发送表情包失败: {}", e),
            }
        }
        _ => String::new(),
    }
}

pub fn run_planner(ctx: &PlannerContext) -> PlannerAction {
    let max_rounds = 10;
    let prompt = crate::prompt::PromptManager::get().raw("planner");

    // 构建工具列表（包含表情包工具）
    let mut tools = vec![tool_reply(), tool_query_memory(), tool_query_person_info(), tool_send_emoji(), tool_finish()];

    // 注入表情包上下文
    let emoji_ctx = crate::emoji_system::get_emoji_context();
    let extra = if emoji_ctx.is_empty() {
        ctx.extra_context.clone()
    } else {
        format!("{}\n\n{}", ctx.extra_context, emoji_ctx)
    };

    let system_prompt = format!("{}\n\n{}", prompt, extra);
    let mut user_content = format!("# 当前消息\n[user_id:{}] {}", ctx.user_id, ctx.user_message);

    for round in 0..max_rounds {
        debug!(round, group_id = ctx.group_id, user_id = ctx.user_id, "planner: round");
        match crate::ai::analyze_with_tools_named(&system_prompt, &user_content, &tools, Some(serde_json::json!("auto"))) {
            Ok((name, args)) => match name.as_str() {
                "reply" => {
                    let ri = args.get("reference_info").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    info!(user_id = ctx.user_id, group_id = ctx.group_id, reference_info = %ri, "planner: reply");
                    return PlannerAction::Reply { user_id: ctx.user_id, group_id: ctx.group_id, message: ctx.user_message.clone(), reference_info: if ri.is_empty() { None } else { Some(ri) } };
                }
                "finish" => {
                    debug!(user_id = ctx.user_id, group_id = ctx.group_id, "planner: finish");
                    return PlannerAction::Silent;
                }
                tool_name => {
                    let result = execute_tool(tool_name, &args, ctx);
                    if !result.is_empty() { user_content.push_str(&format!("\n\n[工具结果: {}]\n{}", tool_name, result)); }
                }
            },
            Err(e) => { warn!(error = %e, round, "planner: AI error"); return PlannerAction::Silent; }
        }
    }
    warn!(max_rounds, "planner: max rounds reached");
    PlannerAction::Silent
}
