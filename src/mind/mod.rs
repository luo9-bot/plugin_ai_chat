//! 心灵模块：意识流、感官、身体与回神
//!
//! 架构纪律（方案书 v2）：
//! - 代码不替她解释世界，只递感官（[`sensation`]）与身体信号
//! - 她的内心活动只由回神路径写入（[`stream::push_inner`]）
//! - 意图堆（[`wake`]）里只有她自己留下的想起；定时器只兑现，不产生意愿
//! - 意识流 append-only，72h 消亡；日记（[`diary`]）与人物档案（[`persons`]）
//!   是睡前整理沉淀下的长存记忆

pub(crate) mod archive;
pub(crate) mod diary;
pub(crate) mod foraging;
pub(crate) mod persons;
pub(crate) mod recall;
pub(crate) mod security;
pub(crate) mod self_model;
pub(crate) mod sensation;
pub(crate) mod social;
pub(crate) mod stream;
pub(crate) mod style;
pub(crate) mod wake;
pub(crate) mod wish;

pub(crate) use persons::PersonFile;
pub(crate) use sensation::{body_signals, transcribe_message};
pub(crate) use stream::{
    StreamEvent, StreamKind, push_acted, push_digested, push_inner, recent, recent_text,
};
pub(crate) use wake::{
    Urgency, WakeKind, WakePlan, add as add_wake_plan, is_night, tick as wake_tick,
};
