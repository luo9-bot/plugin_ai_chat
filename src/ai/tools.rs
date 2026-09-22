use super::types::{FunctionDef, Tool};

// ── Function Call 工具定义 ────────────────────────────────────

/// memory_review: 记忆审查与整理
pub(crate) fn memory_review_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "memory_review".to_string(),
            description: "审查和整理用户记忆".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["keep", "consolidate", "update", "remove"],
                        "description": "操作类型：keep=无需改动, consolidate=合并, update=更新, remove=删除虚假/过时的记忆"
                    },
                    "reason": { "type": "string", "description": "原因" },
                    "updates": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_content": { "type": "string" },
                                "new_content": { "type": "string" },
                                "importance": { "type": "string", "enum": ["permanent", "important", "normal"] }
                            },
                            "required": ["old_content", "new_content", "importance"]
                        },
                        "description": "需要更新的记忆"
                    },
                    "removes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "需要删除的记忆"
                    },
                    "adds": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string" },
                                "importance": { "type": "string", "enum": ["permanent", "important", "normal"] }
                            },
                            "required": ["content", "importance"]
                        },
                        "description": "需要添加的记忆"
                    }
                },
                "required": ["action"]
            }),
        },
    }
}

/// 每日计划生成工具
pub(crate) fn daily_plan_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "daily_plan".to_string(),
            description: "为自己制定今日计划".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "items": {
                            "type": "string"
                        },
                        "description": "今日任务列表，2-4个具体可执行的任务"
                    }
                },
                "required": ["tasks"]
            }),
        },
    }
}

/// 看自己当前还没做完的计划（带 id）
///
/// 计划清单本身会随场景一起递给她（见 `voice::plan_block`），这个工具用于
/// 她想再确认一遍、或清单被截断时按需查全。
pub(crate) fn check_plan_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "check_plan".to_string(),
            description: "看一眼自己今天/本周/本月还没做完的事（带编号）。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    }
}

/// 给自己加一件事（当日）
pub(crate) fn add_plan_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "add_plan".to_string(),
            description: "往今天的计划里加一件事。想做了、答应了、或临时起意都可以加。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": {"type": "string", "description": "要做的事，一句话，别超过 15 字"}
                },
                "required": ["content"]
            }),
        },
    }
}

/// 记下自己做到了哪一步（不动完成状态）
pub(crate) fn note_progress_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "note_progress".to_string(),
            description: "给计划里某件事记一笔进展。事情开了个头、做到一半、卡住了都记一下。"
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "清单里那件事的编号，如 d1"},
                    "progress": {"type": "string", "description": "做到哪一步了，用你自己的话说"}
                },
                "required": ["id", "progress"]
            }),
        },
    }
}

/// 勾掉一件事（完成 / 取消完成）
///
/// 判断由她做：她不靠关键词匹配自己有没有做过，而是看完今天发生的事
/// 之后自己落笔。id 由系统分配，所以"哪一件"没有歧义。
pub(crate) fn finish_plan_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "finish_plan".to_string(),
            description: "把计划里的一件事勾掉（做完了、或者决定不做了），也可以取消勾选。"
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "清单里那件事的编号，如 d1"},
                    "done": {"type": "boolean", "description": "true 勾掉，false 取消勾选"},
                    "note": {"type": "string", "description": "（可选）一句话，比如\"做完了\"\"这次先算了\""}
                },
                "required": ["id", "done"]
            }),
        },
    }
}

/// 从对话中提取 bot 自己需要推进的真实事项。
pub(crate) fn task_progress_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "task_progress".to_string(),
            description: "提取 bot 需要推进的明确约定、承诺或等待事项".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string", "description": "简短的 bot 自身事项" },
                                "next_action": { "type": "string", "description": "下一步的具体自然行动" },
                                "waiting_for_person": { "type": "boolean", "description": "下一步是否必须等当前对话对象回应" }
                            },
                            "required": ["title", "next_action", "waiting_for_person"]
                        },
                        "description": "没有明确事项时为空数组"
                    }
                },
                "required": ["tasks"]
            }),
        },
    }
}

/// 周计划生成工具
pub(crate) fn weekly_plan_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "weekly_plan".to_string(),
            description: "制定本周计划".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "goals": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string", "description": "目标内容" },
                                "target_day": { "type": "string", "enum": ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"], "description": "分配到哪一天" }
                            },
                            "required": ["content", "target_day"]
                        },
                        "description": "本周目标列表，3-5个"
                    }
                },
                "required": ["goals"]
            }),
        },
    }
}

/// 月计划生成工具
pub(crate) fn monthly_plan_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "monthly_plan".to_string(),
            description: "制定本月计划".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "goals": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "本月目标列表，2-4个"
                    }
                },
                "required": ["goals"]
            }),
        },
    }
}
