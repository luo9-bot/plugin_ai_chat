//! Planner 多轮推理引擎
//!
//! 多轮工具调用循环：
//! 1. 可见工具直接暴露给 LLM
//! 2. 延迟工具（deferred）需要先调用 tool_search 发现
//! 3. 发现后在后续轮次变为可用
//!
//! ## 架构说明
//!
//! 核心循环由 `PlannerLoopEngine` 管理，它使用：
//! - `ToolRegistry`：工具的统一注册与可见性控制
//! - `DeferredToolState`：deferred tool 的上下文证据链同步
//! - `PromptRequestBuilder`：结构化消息构造
//!
//! 外部仍通过 `run_planner(ctx)` 调用，保持向后兼容。

mod types;
mod tools;

pub use types::*;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, info, warn};

use crate::runtime::deferred_tools::DeferredToolState;
use crate::runtime::tool_registry::{ToolSpec, ToolVisibility};

// ── 中断机制 ────────────────────────────────────────────────

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

// ── PlannerLoopEngine ────────────────────────────────────────

/// Planner 循环引擎
///
/// 封装多轮推理循环的完整状态，包括工具发现、上下文构造和执行。
pub struct PlannerLoopEngine {
    /// 已发现的 deferred tools（上下文证据链同步）
    deferred_state: DeferredToolState,
    /// 内置工具定义（保持在 planner 本地，不引入外部注册依赖）
    tool_specs: HashMap<String, ToolSpec>,
}

impl PlannerLoopEngine {
    /// 创建引擎并初始化所有工具定义
    pub fn new() -> Self {
        let mut engine = Self {
            deferred_state: DeferredToolState::new(Vec::new()),
            tool_specs: HashMap::new(),
        };
        engine.register_builtin_tools();
        engine
    }

    /// 注册 planner 内置工具
    fn register_builtin_tools(&mut self) {
        // visible 工具
        let visible_tools = vec![
            ToolSpec::visible("reply", "当你判断 bot 应该正式发送一条可见回复时调用。",
                serde_json::json!({"type":"object","properties":{"reference_info":{"type":"string","description":"回复参考信息"}},"required":["reference_info"]})),
            ToolSpec::visible("query_memory", "查询关于某个用户的长期记忆。",
                serde_json::json!({"type":"object","properties":{"user_id":{"type":"integer"},"query":{"type":"string"}},"required":["user_id"]})),
            ToolSpec::visible("query_person_info", "查询用户的人物档案。",
                serde_json::json!({"type":"object","properties":{"user_id":{"type":"integer"}},"required":["user_id"]})),
            ToolSpec::visible("tool_search", "搜索可用的延迟工具。找到的工具会在后续轮次中变为可用。",
                serde_json::json!({"type":"object","properties":{"query":{"type":"string","description":"搜索关键词，如工具名称或功能描述"}},"required":["query"]})),
            ToolSpec::visible("finish", "结束本轮推理，不回复。",
                serde_json::json!({"type":"object","properties":{"reason":{"type":"string"}},"required":["reason"]})),
        ];

        // deferred 工具
        let deferred_tools = vec![
            ToolSpec::deferred("send_sticker",
                "【非必需工具】仅在需要强化情绪表达时偶尔使用。不要每次回复都调用。当文字表达已足够时，应优先使用reply工具。",
                serde_json::json!({"type":"object","properties":{"emotion":{"type":"string","description":"想要表达的情绪，如'开心'、'难过'、'搞笑'"}},"required":["emotion"]}),
                vec!["表情", "sticker", "emoji", "表情包", "图片"]),
        ];

        for spec in visible_tools {
            self.tool_specs.insert(spec.name.clone(), spec);
        }
        for spec in deferred_tools {
            self.tool_specs.insert(spec.name.clone(), spec);
        }

        // 初始化 deferred state
        let deferred_specs: Vec<ToolSpec> = self.tool_specs
            .values()
            .filter(|s| matches!(s.visibility, ToolVisibility::Deferred))
            .cloned()
            .collect();
        self.deferred_state = DeferredToolState::new(deferred_specs);
    }

    /// 获取当前可见工具（visible + 已发现的 deferred）
    fn build_current_tools(&self, discovered: &HashMap<String, usize>, current_round: usize) -> Vec<crate::ai::Tool> {
        let mut tools: Vec<crate::ai::Tool> = Vec::new();

        // 1. 所有 visible 工具
        for spec in self.tool_specs.values() {
            if matches!(spec.visibility, ToolVisibility::Visible) {
                tools.push(spec.to_llm_tool());
            }
        }

        // 2. 已发现的 deferred 工具（TTL 回退机制，作为上下文证据链的过渡）
        for (tool_name, &discovered_round) in discovered.iter() {
            if current_round.saturating_sub(discovered_round) <= 1 {
                if let Some(spec) = self.tool_specs.get(tool_name) {
                    tools.push(spec.to_llm_tool());
                }
            }
        }

        tools
    }

    /// 搜索 deferred tools
    fn search_deferred(&self, query: &str, limit: usize) -> Vec<String> {
        self.deferred_state.search(query, limit)
    }

    /// 批量发现工具
    fn discover_tools(&mut self, names: &[String]) -> Vec<String> {
        self.deferred_state.discover_tools(names)
    }

    /// 构建 deferred tools 提醒文本
    fn build_reminder(&self) -> String {
        self.deferred_state.build_reminder()
    }

    /// 执行工具（保持原有逻辑）
    fn execute_tool(&mut self, name: &str, args: &serde_json::Value, ctx: &PlannerContext) -> String {
        match name {
            "query_memory" => {
                let uid = args.get("user_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                if uid == 0 { return "错误：未提供 user_id".into(); }
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
                let found = self.search_deferred(query, 5);
                if found.is_empty() {
                    "未找到匹配的工具".to_string()
                } else {
                    let newly = self.discover_tools(&found);
                    let mut result_lines = vec![format!("已找到 {} 个延迟工具：", found.len())];
                    for name in &found {
                        if newly.contains(name) {
                            result_lines.push(format!("- {} (新发现)", name));
                        } else {
                            result_lines.push(format!("- {}", name));
                        }
                    }
                    result_lines.join("\n")
                }
            }
            _ => String::new(),
        }
    }
}

// 默认 Planner 循环引擎实例（单次调用内使用）
thread_local! {
    static ENGINE: std::cell::RefCell<PlannerLoopEngine> = std::cell::RefCell::new(PlannerLoopEngine::new());
}

// ── 公开入口 ────────────────────────────────────────────────

/// 运行 Planner
///
/// 使用 `PlannerLoopEngine` 执行多轮工具调用循环，返回决策结果。
/// 保持原有签名不变以兼容外部调用方。
pub fn run_planner(ctx: &PlannerContext) -> PlannerAction {
    let max_rounds = 8;
    let prompt = crate::prompt::PromptManager::get().raw("planner");

    // 注入表情包上下文
    let sticker_ctx = crate::sticker::get_sticker_context();
    let extra = if sticker_ctx.is_empty() {
        ctx.extra_context.clone()
    } else {
        format!("{}\n\n{}", ctx.extra_context, sticker_ctx)
    };

    let system_prompt = format!("{}\n\n{}", prompt, extra);
    let mut user_content = format!("# 当前消息\n[user_id:{}] {}", ctx.user_id, ctx.user_message);

    ENGINE.with(|engine_cell| {
        let mut engine = engine_cell.borrow_mut();
        // 同步 deferred state（先重置已发现工具，本轮从头累计）
        engine.deferred_state.discovered_tool_names.clear();

        // TTL 兼容映射：已被新 DeferredToolState 发现的工具保持可见
        let mut discovered: HashMap<String, usize> = HashMap::new();

        for round in 0..max_rounds {
            if check_interrupt() {
                info!(user_id = ctx.user_id, group_id = ctx.group_id, "planner: interrupted by new message");
                return PlannerAction::Silent;
            }

            debug!(round, group_id = ctx.group_id, user_id = ctx.user_id, "planner: round");

            // 构建工具列表（使用新引擎）
            let tools = engine.build_current_tools(&discovered, round);

            // 构建 deferred tools 提醒
            let reminder = engine.build_reminder();
            let current_content = if reminder.is_empty() {
                user_content.clone()
            } else {
                format!("{}\n\n{}", user_content, reminder)
            };

            match crate::ai::analyze_with_tools_named(
                &system_prompt, &current_content, &tools, Some(serde_json::json!("auto")),
            ) {
                Ok((name, args)) => match name.as_str() {
                    "reply" => {
                        let ri = args.get("reference_info").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        info!(user_id = ctx.user_id, group_id = ctx.group_id, reference_info = %ri, "planner: reply");
                        return PlannerAction::Reply {
                            user_id: ctx.user_id,
                            group_id: ctx.group_id,
                            message: ctx.user_message.clone(),
                            reference_info: if ri.is_empty() { None } else { Some(ri) },
                        };
                    }
                    "finish" => {
                        debug!(user_id = ctx.user_id, group_id = ctx.group_id, "planner: finish");
                        return PlannerAction::Silent;
                    }
                    tool_name => {
                        let result = engine.execute_tool(tool_name, &args, ctx);
                        if !result.is_empty() {
                            // 记录 TTL 兼容的发现
                            if tool_name == "tool_search" {
                                for name in engine.deferred_state.discovered_tool_names.iter() {
                                    discovered.insert(name.clone(), round);
                                }
                                user_content.push_str(&format!("\n\n[工具结果: {}]\n{}", tool_name, result));
                            } else {
                                user_content.push_str(&format!("\n\n[工具执行完毕: {}]\n{}\n\n请根据以上结果决定是回复还是结束对话。", tool_name, result));
                            }
                        }
                    }
                },
                Err(e) => {
                    warn!(error = %e, round, "planner: AI error");
                    return PlannerAction::Silent;
                }
            }
        }

        warn!(max_rounds, "planner: max rounds reached");
        PlannerAction::Silent
    })
}
