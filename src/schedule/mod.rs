//! 日程计划：她自己的事
//!
//! 一条权威路径：生成（日/周/月各一次）→ 她看见带 id 的清单 → 她判断并落笔。
//! `config` 管作息时间表，`store` 管计划本身的模型与存储。

pub mod config;
pub mod store;

pub use config::{DailySchedule, ScheduleConfig, ScheduledEvent};
pub use store::{
    GeneratedItem, Plan, PlanItem, SetStatusOutcome, Timeframe, add_own_item, ensure_plan, find,
    open_items, open_items_all, plan_of, push_history, render_open_items, replace_items,
    set_status, today_week_items,
};
