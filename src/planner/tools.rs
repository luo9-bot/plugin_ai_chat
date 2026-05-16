//! Planner 工具定义（旧版函数式定义，The new ToolSpec system is in PlannerLoopEngine）
//!
//! 这些函数保留供外部兼容引用，新代码请使用 PlannerLoopEngine。

#![allow(dead_code)]

use crate::ai::{Tool, FunctionDef};

pub fn tool_reply() -> Tool {
    Tool { tool_type: "function".into(), function: FunctionDef {
        name: "reply".into(),
        description: "只有在你已从记忆或上下文中获得足够信息，确定 bot 应该回复时才调用。信息不足时请使用 finish 保持沉默。".into(),
        parameters: serde_json::json!({"type":"object","properties":{"reference_info":{"type":"string","description":"回复参考信息（基于你已获取到的信息简要说明为什么回复）"}},"required":["reference_info"]}),
    }}
}

pub fn tool_query_memory() -> Tool {
    Tool { tool_type: "function".into(), function: FunctionDef {
        name: "query_memory".into(),
        description: "查询关于某个用户的长期记忆。可能返回空结果——如果返回为空说明没有相关信息，不要编造。".into(),
        parameters: serde_json::json!({"type":"object","properties":{"user_id":{"type":"integer"},"query":{"type":"string","description":"要查询的内容，如用户的喜好、说过的话等"}},"required":["user_id"]}),
    }}
}

pub fn tool_query_person_info() -> Tool {
    Tool { tool_type: "function".into(), function: FunctionDef {
        name: "query_person_info".into(),
        description: "查询用户的人物档案。".into(),
        parameters: serde_json::json!({"type":"object","properties":{"user_id":{"type":"integer"}},"required":["user_id"]}),
    }}
}

pub fn tool_finish() -> Tool {
    Tool { tool_type: "function".into(), function: FunctionDef {
        name: "finish".into(),
        description: "结束本轮推理，不回复。当你缺乏足够信息无法给出有意义回复时，请选择 finish 而不是胡乱回应。".into(),
        parameters: serde_json::json!({"type":"object","properties":{"reason":{"type":"string","description":"不回复的原因（如：信息不足、话题不相关、不需要回复等）"}},"required":["reason"]}),
    }}
}

pub fn tool_send_sticker() -> Tool {
    Tool { tool_type: "function".into(), function: FunctionDef {
        name: "send_sticker".into(),
        description: "【非必需工具】仅在需要强化情绪表达时偶尔使用。不要每次回复都调用。当文字表达已足够时，应优先使用reply工具。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "emotion": {
                    "type": "string",
                    "description": "想要表达的情绪，如'开心'、'难过'、'搞笑'"
                }
            },
            "required": ["emotion"]
        }),
    }}
}

pub fn tool_tool_search() -> Tool {
    Tool { tool_type: "function".into(), function: FunctionDef {
        name: "tool_search".into(),
        description: "搜索可用的延迟工具。找到的工具会在后续轮次中变为可用。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "搜索关键词，如工具名称或功能描述"
                }
            },
            "required": ["query"]
        }),
    }}
}