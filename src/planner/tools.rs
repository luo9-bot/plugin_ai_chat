//! Planner 工具定义

use crate::ai::{Tool, FunctionDef};

pub fn tool_reply() -> Tool {
    Tool { tool_type: "function".into(), function: FunctionDef {
        name: "reply".into(),
        description: "当你判断 bot 应该正式发送一条可见回复时调用。".into(),
        parameters: serde_json::json!({"type":"object","properties":{"reference_info":{"type":"string","description":"回复参考信息"}},"required":["reference_info"]}),
    }}
}

pub fn tool_query_memory() -> Tool {
    Tool { tool_type: "function".into(), function: FunctionDef {
        name: "query_memory".into(),
        description: "查询关于某个用户的长期记忆。".into(),
        parameters: serde_json::json!({"type":"object","properties":{"user_id":{"type":"integer"},"query":{"type":"string"}},"required":["user_id"]}),
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
        description: "结束本轮推理，不回复。".into(),
        parameters: serde_json::json!({"type":"object","properties":{"reason":{"type":"string"}},"required":["reason"]}),
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