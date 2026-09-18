//! 运行时状态
//!
//! 按**谁拥有它**分两层，而不是按"是不是状态"分：
//! - [`GateState`]（谁在对话、谁被拉黑）是进程级的，后台线程也要能改并立即生效；
//! - [`BatchBuffer`]（待处理消息批次）只属于 1ms 主循环线程。
//!
//! 两者早先共用一个 `thread_local`，后台改动因此落在自己线程的副本上。

mod local;
mod shared;

pub use local::{BatchBuffer, GateState, MessageBatch, TakenBatch};
pub use shared::{CtxKey, SharedState, UserContext};
