use super::types::{Tool, FunctionDef};

// ── Function Call 工具定义 ────────────────────────────────────

/// decide_reply: 判断是否回复群消息
pub fn decide_reply_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "decide_reply".to_string(),
            description: "判断是否回复群聊中的消息".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "reply": {
                        "type": "boolean",
                        "description": "是否回复"
                    },
                    "reason": {
                        "type": "string",
                        "description": "简短原因"
                    }
                },
                "required": ["reply"]
            }),
        },
    }
}

/// 批量决策：从多条消息中选择值得回复的用户
pub fn batch_decide_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "batch_decide".to_string(),
            description: "从群聊的多条消息中，选择最值得回复的用户".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "reply_to": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "user_id": { "type": "integer", "description": "要回复的用户ID" },
                                "reason": { "type": "string", "description": "回复原因" }
                            },
                            "required": ["user_id"]
                        },
                        "description": "要回复的用户列表，按优先级排序。大部分情况应该为空或只包含1个用户"
                    }
                },
                "required": ["reply_to"]
            }),
        },
    }
}

/// post_analyze: 记忆提取 + 情绪分析 + 记忆纠错
pub fn post_analyze_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "post_analyze".to_string(),
            description: "分析对话，提取记忆、分析情绪、检测纠错".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "memories": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string", "description": "记忆内容" },
                                "importance": { "type": "string", "enum": ["permanent", "important", "normal"], "description": "重要性" }
                            },
                            "required": ["content", "importance"]
                        },
                        "description": "值得长期记忆的信息"
                    },
                    "emotion": {
                        "type": "string",
                        "enum": ["neutral", "happy", "sad", "thinking", "surprised", "angry", "shy", "worried", "tired", "excited"],
                        "description": "用户情绪状态"
                    },
                    "intensity": {
                        "type": "number",
                        "description": "情绪强度 0.0~1.0"
                    },
                    "corrections": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old": { "type": "string", "description": "需要修正的旧记忆内容" },
                                "new": { "type": "string", "description": "修正后的正确内容" },
                                "target": { "type": "string", "enum": ["user", "self"], "description": "修正目标" }
                            },
                            "required": ["old", "new", "target"]
                        },
                        "description": "记忆纠错"
                    },
                    "concerns": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string", "description": "担忧内容" },
                                "category": { "type": "string", "enum": ["social", "task", "emotional", "self"], "description": "担忧类别" }
                            },
                            "required": ["content", "category"]
                        },
                        "description": "从对话中产生的担忧，没有则为空数组"
                    },
                    "deliberations": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string", "description": "考量内容" }
                            },
                            "required": ["content"]
                        },
                        "description": "从对话中积累的内部考量，没有则为空数组"
                    }
                },
                "required": ["memories", "emotion", "intensity", "corrections"]
            }),
        },
    }
}

/// review_conversation: 审查群聊对话提取记忆
pub fn review_conversation_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "review_conversation".to_string(),
            description: "审查群聊对话，提取值得记忆的信息和情绪".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "relevant": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "user_id": { "type": "integer", "description": "用户ID" },
                                "memory": { "type": "string", "description": "值得记住的内容" },
                                "importance": { "type": "string", "enum": ["normal", "important", "permanent"], "description": "重要性" }
                            },
                            "required": ["user_id", "memory", "importance"]
                        },
                        "description": "值得记忆的信息"
                    },
                    "emotion": {
                        "type": "object",
                        "properties": {
                            "state": { "type": "string", "enum": ["neutral", "happy", "sad", "thinking", "surprised", "angry", "shy", "worried", "tired", "excited"] },
                            "intensity": { "type": "number", "description": "情绪强度 0.0~1.0" }
                        },
                        "required": ["state", "intensity"],
                        "description": "情绪状态"
                    }
                },
                "required": ["relevant", "emotion"]
            }),
        },
    }
}

/// self_reflect: 自我反思
pub fn self_reflect_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "self_reflect".to_string(),
            description: "产生内心想法，可选择是否分享到群聊".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "thoughts": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string", "description": "内心想法" },
                                "category": { "type": "string", "enum": ["reflection", "experience", "plan", "feeling"], "description": "想法类别" }
                            },
                            "required": ["content", "category"]
                        },
                        "description": "内心想法列表"
                    },
                    "share": {
                        "type": "object",
                        "properties": {
                            "should_share": { "type": "boolean", "description": "是否分享到群聊" },
                            "content": { "type": "string", "description": "分享的内容" },
                            "target_group_id": { "type": "integer", "description": "目标群号，0表示不分享" }
                        },
                        "required": ["should_share", "content", "target_group_id"],
                        "description": "主动分享设置"
                    },
                    "concerns": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string", "description": "担忧内容" },
                                "category": { "type": "string", "enum": ["social", "task", "emotional", "self"], "description": "担忧类别" }
                            },
                            "required": ["content", "category"]
                        },
                        "description": "反思中产生的担忧"
                    },
                    "deliberations": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string", "description": "考量内容" }
                            },
                            "required": ["content"]
                        },
                        "description": "反思中产生的考量"
                    }
                },
                "required": ["thoughts", "share"]
            }),
        },
    }
}

/// memory_review: 记忆审查与整理
pub fn memory_review_tool() -> Tool {
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

/// 主动消息生成工具
pub fn proactive_message_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "proactive_message".to_string(),
            description: "决定是否要主动说话，以及要说什么".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "skip": {
                        "type": "boolean",
                        "description": "是否跳过不说话。true = 没什么好说的，不发送任何消息。false = 有话说。"
                    },
                    "distinct_from_recent": {
                        "type": "string",
                        "description": "必填。对比「你自己刚才说过的话」，解释你即将发送的消息和最近发过的有什么不同。如果内容基本一样或只是换了个说法，请设置 skip=true 而不是填这个字段。"
                    },
                    "message": {
                        "type": "string",
                        "description": "要发送的消息内容，简短口语化。只有 skip=false 时才需要填。"
                    }
                },
                "required": ["skip", "distinct_from_recent"]
            }),
        },
    }
}

/// 每日计划生成工具
pub fn daily_plan_tool() -> Tool {
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

/// 心理状态生成工具 (担忧 + 要考量)
pub fn mental_state_generate_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDef {
            name: "mental_state_generate".to_string(),
            description: "从对话中生成担忧和考量".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "concerns": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string", "description": "担忧内容" },
                                "category": { "type": "string", "enum": ["social", "task", "emotional", "self"], "description": "担忧类别" }
                            },
                            "required": ["content", "category"]
                        },
                        "description": "担忧列表，没有则为空数组"
                    },
                    "deliberations": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string", "description": "考量内容" }
                            },
                            "required": ["content"]
                        },
                        "description": "考量列表，没有则为空数组"
                    }
                },
                "required": ["concerns", "deliberations"]
            }),
        },
    }
}

/// 周计划生成工具
pub fn weekly_plan_tool() -> Tool {
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
pub fn monthly_plan_tool() -> Tool {
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
