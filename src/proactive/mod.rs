mod generate;
pub mod motivation;
mod runtime;
mod trigger;

// ── re-exports ────────────────────────────────────────────────

// runtime.rs
pub use runtime::{
    DateReminder, ProactiveState, RuntimeConfig, add_date_reminder, can_send_hurt_check_in,
    get_group_last_sent, load_state, private_contact_interval, record_hurt_check_in,
    record_private_user_reply, record_sent, record_user_reply, set_enabled, set_interval,
    set_quiet_hours, user_count,
};

// trigger.rs
pub use trigger::{check_group_atmosphere, check_proactive_messages};
