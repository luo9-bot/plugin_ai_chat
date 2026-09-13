//! 表达学习系统
//!
//! 从群聊消息中学习语言风格（写入侧）；读取侧尚未接线。

mod extract;
mod store;

pub use extract::*;
pub use store::*;
