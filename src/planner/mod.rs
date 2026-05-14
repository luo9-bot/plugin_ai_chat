//! Planner 多轮推理引擎
//!
//! 多轮工具调用循环：
//! 1. 可见工具直接暴露给 LLM
//! 2. 延迟工具（deferred）需要先调用 tool_search 发现
//! 3. 发现后在下一轮变为可用

mod types;
mod tools;

pub use types::*;
use tools::*;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, info, warn};

/// Planner 中断标志：新消息到达时设置，中断当前推理
static INTERRUPT_FLAG: AtomicBool = AtomicBool::new(false);

/// 请求中断当前 Planner 推理（由消息到达时调用）
pub fn request_interrupt() {
    INTERRUPT_FLAG.store(true, Ordering::Relaxed);
}

/// 检查并清除中断标志
fn check_interrupt() -> bool {
    INTERRUPT_FLAG.swap(false, Ordering::Relaxed)
}

/// 工具可见性
#[derive(Debug, Clone, PartialEq)]
pub enum ToolVisibility {
    /// 始终可见
    Visible,
    /// 需要 tool_search 发现后才可见
    Deferred,
}

/// 延迟工具定义
struct DeferredTool {
    name: &'static str,
    description: &'static str,
    keywords: Vec<&'static str>,
}

/// 获取所有延迟工具列表
fn get_deferred_tools() -> Vec<DeferredTool> {
    vec![
        DeferredTool {
            name: "send_sticker",
            description: "发送表情包。当你想用表情包表达情绪时调用。",
            keywords: vec!["表情", "sticker", "emoji", "表情包", "图片"],
        },
    ]
}

/// 搜索延迟工具
fn search_deferred_tools(query: &str, discovered: &HashMap<String, usize>) -> Vec<String> {
    let query_lower = query.to_lowercase();
    let query_terms: Vec<&str> = query_lower.split(['_', '-', ' ']).collect();

    let mut scored: Vec<(&str, i32)> = Vec::new();

    for dt in get_deferred_tools() {
        if discovered.contains_key(dt.name) {
            continue; // 已发现，跳过
        }

        let name_lower = dt.name.to_lowercase();
        let desc_lower = dt.description.to_lowercase();
        let mut score = 0i32;

        // 完全匹配
        if query_lower == name_lower { score += 1000; }
        // 前缀匹配
        if name_lower.starts_with(&query_lower) { score += 300; }
        // 子串匹配（名称）
        if name_lower.contains(&query_lower) { score += 200; }
        // 子串匹配（描述）
        if desc_lower.contains(&query_lower) { score += 100; }

        // 关键词匹配
        for &keyword in &dt.keywords {
            if query_lower.contains(keyword) { score += 50; }
        }

        // 逐词匹配
        for term in &query_terms {
            if name_lower.contains(term) { score += 25; }
            if desc_lower.contains(term) { score += 10; }
        }

        if score > 0 {
            scored.push((dt.name, score));
        }
    }

    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.into_iter().map(|(name, _)| name.to_string()).collect()
}

/// 构建延迟工具的系统提醒文本
fn build_deferred_tools_reminder(discovered: &HashMap<String, usize>) -> String {
    let all = get_deferred_tools();
    let not_discovered: Vec<&DeferredTool> = all
        .iter()
        .filter(|dt| !discovered.contains_key(dt.name))
        .collect();

    if not_discovered.is_empty() {
        return String::new();
    }

    let mut lines = vec!["<system-reminder>".to_string()];
    lines.push("以下工具已注册但需要先调用 tool_search 发现后才能使用：".to_string());
    for dt in &not_discovered {
        lines.push(format!("- {}: {}", dt.name, dt.description));
    }
    lines.push("</system-reminder>".to_string());
    lines.join("\n")
}

fn execute_tool(name: &str, args: &serde_json::Value, ctx: &PlannerContext, discovered: &mut HashMap<String, usize>, current_round: usize) -> String {
    match name {
        "query_memory" => {
            let uid = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            if uid == 0 { return "错误：未提供 user_id".into(); }
            // 使用 BM25 检索最相关的记忆（而非全量注入）
            let results = crate::memory::search_memories(uid, query, 10);
            if results.is_empty() {
                format!("用户 {} 没有相关记忆（查询: {}）", uid, query)
            } else {
                let mut output = format!("用户 {} 的相关记忆：\n", uid);
                for r in &results {
                    output.push_str(&format!("- {}\n", r.content));
                }
                output
            }
        }
        "query_person_info" => {
            let uid = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
            if uid == 0 { return "错误：未提供 user_id".into(); }
            let m = crate::memory::get_context(uid);
            if m.is_empty() { format!("用户 {} 没有已知的人物信息", uid) } else { m }
        }
        "send_sticker" => {
            let emotion = args.get("emotion").and_then(|v| v.as_str()).unwrap_or("开心");
            let context = format!("用户消息: {}", ctx.user_message);
            match crate::sticker::send_sticker(ctx.group_id, ctx.user_id, emotion, &context, &[]) {
                Ok(desc) => format!("已发送表情包: {}", desc),
                Err(e) => format!("发送表情包失败: {}", e),
            }
        }
        "tool_search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let found = search_deferred_tools(query, discovered);
            if found.is_empty() {
                "未找到匹配的工具".to_string()
            } else {
                let mut result_lines = vec!["已发现以下工具，后续轮次可直接使用：".to_string()];
                for name in &found {
                    discovered.insert(name.clone(), current_round);
                    result_lines.push(format!("- {}", name));
                }
                result_lines.join("\n")
            }
        }
        _ => String::new(),
    }
}

/// Deferred Tool 有效轮数（超过此轮数自动回退）
///
/// 工具在 tool_search 发现时记录 discovered_round = N，
/// 下一轮 (N+1) 起可见，持续 DISCOVERY_TTL_ROUNDS 轮后过期。
const DISCOVERY_TTL_ROUNDS: usize = 1;

/// 构建当前可见的工具列表
///
/// 只包含在 DISCOVERY_TTL_ROUNDS 范围内发现的延迟工具
fn build_visible_tools(discovered: &HashMap<String, usize>, current_round: usize) -> Vec<crate::ai::Tool> {
    let mut tools = vec![
        tool_reply(),
        tool_query_memory(),
        tool_query_person_info(),
        tool_tool_search(),
        tool_finish(),
    ];

    for (tool_name, &discovered_round) in discovered.iter() {
        if current_round - discovered_round <= DISCOVERY_TTL_ROUNDS {
            debug!("planner: tool {} discovered at round {} (expires at round {}), current {} -> visible",
                tool_name, discovered_round, discovered_round + DISCOVERY_TTL_ROUNDS, current_round);
            match tool_name.as_str() {
                "send_sticker" => tools.push(tool_send_sticker()),
                _ => {}
            }
        } else {
            debug!("planner: tool {} expired (discovered round {}, current {})",
                tool_name, discovered_round, current_round);
        }
    }
    tools
}

pub fn run_planner(ctx: &PlannerContext) -> PlannerAction {
    let max_rounds = 8;
    let prompt = crate::prompt::PromptManager::get().raw("planner");

    // 已发现的延迟工具 (tool_name -> 发现时的轮次)
    let mut discovered: HashMap<String, usize> = HashMap::new();

    // 注入表情包上下文
    let sticker_ctx = crate::sticker::get_sticker_context();
    let extra = if sticker_ctx.is_empty() {
        ctx.extra_context.clone()
    } else {
        format!("{}\n\n{}", ctx.extra_context, sticker_ctx)
    };

    let system_prompt = format!("{}\n\n{}", prompt, extra);
    let mut user_content = format!("# 当前消息\n[user_id:{}] {}", ctx.user_id, ctx.user_message);

    for round in 0..max_rounds {
        // 检查中断标志（新消息到达时由外部设置）
        if check_interrupt() {
            info!(user_id = ctx.user_id, group_id = ctx.group_id, "planner: interrupted by new message");
            return PlannerAction::Silent;
        }

        debug!(round, group_id = ctx.group_id, user_id = ctx.user_id, "planner: round");

        // 构建当前可见工具列表（自动回退过期的延迟工具）
        let tools = build_visible_tools(&discovered, round);

        // 注入延迟工具提醒（如果有未发现的工具）
        let reminder = build_deferred_tools_reminder(&discovered);
        let current_content = if reminder.is_empty() {
            user_content.clone()
        } else {
            format!("{}\n\n{}", user_content, reminder)
        };

        match crate::ai::analyze_with_tools_named(&system_prompt, &current_content, &tools, Some(serde_json::json!("auto"))) {
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
                    let result = execute_tool(tool_name, &args, ctx, &mut discovered, round);
                    if !result.is_empty() {
                        // 非搜索工具执行后，追加结果并提示下一轮应 reply/finish
                        if tool_name == "tool_search" {
                            user_content.push_str(&format!("\n\n[工具结果: {}]\n{}", tool_name, result));
                        } else {
                            // 动作工具执行完毕，直接进入 reply 轮次以生成文字回复
                            user_content.push_str(&format!("\n\n[工具执行完毕: {}]\n{}\n\n请根据以上结果决定是回复还是结束对话。", tool_name, result));
                        }
                    }
                }
            },
            Err(e) => { warn!(error = %e, round, "planner: AI error"); return PlannerAction::Silent; }
        }
    }
    warn!(max_rounds, "planner: max rounds reached");
    PlannerAction::Silent
}
