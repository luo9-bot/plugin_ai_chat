mod runtime;
mod generate;
mod trigger;

// ── re-exports ────────────────────────────────────────────────

// runtime.rs
pub use runtime::{
    ProactiveState, DateReminder, RuntimeConfig,
    user_count, record_user_reply, record_sent, add_date_reminder,
    set_enabled, set_quiet_hours, set_interval,
    get_group_last_sent,
};

// trigger.rs
pub use trigger::{check_proactive_messages, check_group_atmosphere};
