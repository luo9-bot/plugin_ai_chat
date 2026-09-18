mod detect;
mod state;

// ── re-exports ────────────────────────────────────────────────

// state.rs
pub(crate) use state::{
    EmotionState, EmotionType, decay_many, get_state, update_state, user_count,
};

// detect.rs
pub(crate) use detect::analyze_user_message;
