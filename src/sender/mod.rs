//! 消息发送：分段处理、打字延迟、安全检查

mod segments;
mod send;

pub use segments::*;
pub use send::*;
