mod detect;
mod state;

// ── re-exports ────────────────────────────────────────────────

// state.rs
pub use state::{EmotionState, EmotionType, decay_many, get_state, update_state, user_count};

// detect.rs
pub use detect::analyze_user_message;
