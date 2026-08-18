mod runtime;
mod generate;
mod trigger;
pub mod motivation;

// ── re-exports ────────────────────────────────────────────────

// runtime.rs
pub use runtime::{
    ProactiveState, DateReminder, RuntimeConfig, load_state,
    user_count, record_user_reply, record_private_user_reply, record_sent, add_date_reminder,
    set_enabled, set_quiet_hours, set_interval,
    get_group_last_sent, private_contact_interval, can_send_hurt_check_in,
    record_hurt_check_in,
};

// trigger.rs
pub use trigger::{check_proactive_messages, check_group_atmosphere};
