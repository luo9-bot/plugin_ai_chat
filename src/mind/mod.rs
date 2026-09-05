//! 心灵模块：意识流、感官与身体
//!
//! 架构纪律（方案书 v2）：
//! - 代码不替她解释世界，只递感官（[`sensation`]）与身体信号
//! - 她的内心活动只由回神路径写入（[`stream::push_inner`]）
//! - 意识流 append-only，72h 消亡，只有睡前整理的沉淀物长存

pub mod sensation;
pub mod stream;

pub use sensation::{SensoryPacket, body_signals, transcribe_message, transcribe_world};
pub use stream::{
    KEEP_DAYS, StreamEvent, StreamKind, cleanup, push_acted, push_digested, push_inner,
    push_sensation, recent, recent_text,
};
