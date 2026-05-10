use serde::{Deserialize, Serialize};

// ── Function Call (Tool Use) 相关结构体 ────────────────────────

#[derive(Serialize, Clone)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

#[derive(Serialize, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub function: ToolCallFunction,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

// ── API 请求/响应结构体 ──────────────────────────────────────

#[derive(Serialize)]
pub(crate) struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub frequency_penalty: f64,
    pub presence_penalty: f64,
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    /// DeepSeek 思考模式控制：{"type": "disabled"} 禁用思考，提升 tool_calls 返回率
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct ChatMessage {
    pub role: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// DeepSeek V4 等模型的推理内容，后续请求必须回传
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ChatResponse {
    pub choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
pub(crate) struct ChatChoice {
    pub message: ChatMessage,
}

/// 记忆纠错条目
pub struct MemoryCorrection {
    pub old: String,   // 需要修正的旧内容 (模糊匹配)
    pub new: String,   // 修正后的正确内容 (空 = 删除)
    pub target: String, // "user" | "self"
}

/// 后处理分析结果
pub struct PostAnalysis {
    pub memories: Vec<(String, String)>, // (content, importance)
    pub emotion: String,
    pub intensity: f32,
    pub corrections: Vec<MemoryCorrection>,
    pub concerns: Vec<(String, String)>,      // (content, category)
    pub deliberations: Vec<String>,            // content
}
