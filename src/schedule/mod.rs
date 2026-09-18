//! 日程计划：她自己的事
//!
//! 一条权威路径：生成（日/周/月各一次）→ 她看见带 id 的清单 → 她判断并落笔。
//! `store` 管计划本身的模型与存储。

pub mod store;

pub use store::{
    GeneratedItem, SetStatusOutcome, Timeframe, add_own_item, ensure_plan, plan_of, push_history,
    render_open_items, replace_items, set_status,
};
