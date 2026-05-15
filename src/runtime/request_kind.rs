use std::fmt;

/// 请求类型枚举
///
/// 每种请求类型使用不同的 Prompt、上下文窗口和工具集合。
/// 这是 Prompt Runtime 化的基础标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestKind {
    /// 主规划器：分析对话、调用工具、决定回复
    Planner,
    /// 时序门控：判断当前是否需要发言
    TimingGate,
    /// 回复生成器：将规划结果转为自然语言回复
    Replyer,
    /// 表情包选择：用 VLM 选择最合适表情
    StickerSelector,
    /// 记忆审查：批量回顾对话提取记忆
    MemoryReview,
    /// 主动消息：根据上下文主动发起对话
    Proactive,
}

impl fmt::Display for RequestKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Planner => write!(f, "planner"),
            Self::TimingGate => write!(f, "timing_gate"),
            Self::Replyer => write!(f, "replyer"),
            Self::StickerSelector => write!(f, "sticker_selector"),
            Self::MemoryReview => write!(f, "memory_review"),
            Self::Proactive => write!(f, "proactive"),
        }
    }
}
