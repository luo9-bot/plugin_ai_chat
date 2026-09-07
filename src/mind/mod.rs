//! 心灵模块：意识流、感官、身体与回神
//!
//! 架构纪律（方案书 v2）：
//! - 代码不替她解释世界，只递感官（[`sensation`]）与身体信号
//! - 她的内心活动只由回神路径写入（[`stream::push_inner`]）
//! - 意图堆（[`wake`]）里只有她自己留下的想起；定时器只兑现，不产生意愿
//! - 意识流 append-only，72h 消亡；日记（[`diary`]）与人物档案（[`persons`]）
//!   是睡前整理沉淀下的长存记忆

pub mod archive;
pub mod diary;
pub mod persons;
pub mod recall;
pub mod security;
pub mod self_model;
pub mod sensation;
pub mod social;
pub mod stream;
pub mod style;
pub mod wake;

pub use diary::DiaryEntry;
pub use persons::{PersonFile, PersonSeed};
pub use sensation::{SensoryPacket, body_signals, transcribe_message, transcribe_world};
pub use social::{SocialState, TopicThread};
pub use stream::{
    KEEP_DAYS, StreamEvent, StreamKind, cleanup, push_acted, push_digested, push_inner,
    push_sensation, recent, recent_text,
};
pub use wake::{
    Urgency, WakeKind, WakePlan, WakeProduct, add as add_wake_plan, is_night, tick as wake_tick,
};
