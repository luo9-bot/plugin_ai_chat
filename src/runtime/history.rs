use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 工具调用记录（与 AI 返回的 tool_call 对应）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// 工具执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultRecord {
    pub tool_call_id: String,
    pub tool_name: String,
    pub success: bool,
    pub content: String,
    pub structured_content: Option<serde_json::Value>,
}

/// 结构化上下文消息
///
/// 替代当前 `Vec<(String, String)>` 的扁平历史表示。
/// 每条消息携带角色信息与可选的工具调用/结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextMessage {
    User {
        user_id: u64,
        content: String,
    },
    Assistant {
        content: String,
        tool_calls: Vec<ToolCallRecord>,
    },
    ToolResult {
        tool_name: String,
        tool_call_id: String,
        success: bool,
        content: String,
    },
    /// 系统注入的参考信息（记忆、行话等），不占用常规上下文槽位
    Reference {
        source: String,
        content: String,
    },
}

impl ContextMessage {
    /// 获取消息的文本内容（用于落盘和摘要）
    pub fn text_content(&self) -> &str {
        match self {
            Self::User { content, .. }
            | Self::Assistant { content, .. }
            | Self::ToolResult { content, .. }
            | Self::Reference { content, .. } => content,
        }
    }

    /// 此消息是否计入上下文窗口计数
    pub fn count_in_context(&self) -> bool {
        matches!(self, Self::User { .. } | Self::Assistant { .. })
    }
}

/// 从旧格式 `Vec<(角色, 内容)>` 转换为结构化历史
pub fn from_flat_history(flat: &[(String, String)]) -> Vec<ContextMessage> {
    let mut result = Vec::with_capacity(flat.len());
    for (role, content) in flat {
        match role.as_str() {
            "user" => result.push(ContextMessage::User {
                user_id: 0,
                content: content.clone(),
            }),
            "assistant" => result.push(ContextMessage::Assistant {
                content: content.clone(),
                tool_calls: Vec::new(),
            }),
            _ => result.push(ContextMessage::Reference {
                source: role.clone(),
                content: content.clone(),
            }),
        }
    }
    result
}

/// 收集上下文中所有 tool_search 的 call_id
pub fn collect_tool_search_call_ids(history: &[ContextMessage]) -> Vec<String> {
    let mut ids = Vec::new();
    for msg in history {
        if let ContextMessage::Assistant { tool_calls, .. } = msg {
            for tc in tool_calls {
                if tc.tool_name == "tool_search" {
                    ids.push(tc.call_id.clone());
                }
            }
        }
    }
    ids
}

/// 从消息中收集所有 tool_search call_id（`HashMap` 版本，快速查重）
pub fn collect_tool_search_call_id_set(history: &[ContextMessage]) -> HashMap<String, bool> {
    let mut map = HashMap::new();
    for msg in history {
        if let ContextMessage::Assistant { tool_calls, .. } = msg {
            for tc in tool_calls {
                if tc.tool_name == "tool_search" {
                    map.insert(tc.call_id.clone(), true);
                }
            }
        }
    }
    map
}
