use crate::ai::ChatMessage;
use crate::runtime::history::ContextMessage;
use crate::runtime::request_kind::RequestKind;

/// 请求构造器：将结构化上下文和工具定义转为 LLM 消息序列
///
/// 替代当前的 `format!()` 字符串拼接方式。
pub struct PromptRequestBuilder {
    kind: RequestKind,
    messages: Vec<ChatMessage>,
    selection_reason: Option<String>,
}

impl PromptRequestBuilder {
    pub fn new(kind: RequestKind) -> Self {
        Self {
            kind,
            messages: Vec::new(),
            selection_reason: None,
        }
    }

    /// 添加 system 消息
    pub fn system(mut self, content: impl Into<String>) -> Self {
        self.messages.push(ChatMessage {
            role: "system".into(),
            content: Some(content.into()),
            tool_calls: None,
            reasoning_content: None,
        });
        self
    }

    /// 添加 user 消息
    pub fn user(mut self, content: impl Into<String>) -> Self {
        self.messages.push(ChatMessage {
            role: "user".into(),
            content: Some(content.into()),
            tool_calls: None,
            reasoning_content: None,
        });
        self
    }

    /// 添加一条按角色构造的消息
    pub fn message(mut self, role: impl Into<String>, content: impl Into<String>) -> Self {
        self.messages.push(ChatMessage {
            role: role.into(),
            content: Some(content.into()),
            tool_calls: None,
            reasoning_content: None,
        });
        self
    }

    /// 注入上下文选择说明
    pub fn with_selection_reason(mut self, reason: impl Into<String>) -> Self {
        self.selection_reason = Some(reason.into());
        self
    }

    /// 将结构化历史消息追加为 user/assistant/tool 消息序列
    pub fn append_history(mut self, history: &[ContextMessage]) -> Self {
        for msg in history {
            match msg {
                ContextMessage::User { content, .. } => {
                    self.messages.push(ChatMessage {
                        role: "user".into(),
                        content: Some(content.clone()),
                        tool_calls: None,
                        reasoning_content: None,
                    });
                }
                ContextMessage::Assistant { content, tool_calls } => {
                    let assistant_tool_calls = if tool_calls.is_empty() {
                        None
                    } else {
                        Some(
                            tool_calls
                                .iter()
                                .map(|tc| crate::ai::ToolCall {
                                    function: crate::ai::ToolCallFunction {
                                        name: tc.tool_name.clone(),
                                        arguments: tc.arguments.to_string(),
                                    },
                                })
                                .collect(),
                        )
                    };
                    self.messages.push(ChatMessage {
                        role: "assistant".into(),
                        content: Some(content.clone()),
                        tool_calls: assistant_tool_calls,
                        reasoning_content: None,
                    });
                }
                ContextMessage::ToolResult {
                    tool_name: _,
                    tool_call_id: _,
                    content,
                    ..
                } => {
                    self.messages.push(ChatMessage {
                        role: "tool".into(),
                        content: Some(
                            if content.is_empty() {
                                "(工具执行完毕)".into()
                            } else {
                                content.clone()
                            },
                        ),
                        tool_calls: None,
                        reasoning_content: None,
                    });
                }
                ContextMessage::Reference { content, .. } => {
                    self.messages.push(ChatMessage {
                        role: "system".into(),
                        content: Some(format!("[参考信息]\n{}", content)),
                        tool_calls: None,
                        reasoning_content: None,
                    });
                }
            }
        }
        self
    }

    /// 构建完成，返回消息序列
    #[allow(dead_code)]
    pub(crate) fn build(self) -> Vec<ChatMessage> {
        self.messages
    }

    pub fn kind(&self) -> RequestKind {
        self.kind
    }
}
