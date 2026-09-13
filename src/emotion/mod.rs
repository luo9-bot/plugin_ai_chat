mod context;
mod detect;
mod state;

// ── re-exports ────────────────────────────────────────────────

// state.rs
pub use state::{
    EmotionState, EmotionTrigger, EmotionType, TriggerType, decay, describe, get_state,
    update_state, user_count,
};

// detect.rs
pub use detect::{ai_analyze, analyze_user_message, update_from_analysis};

// context.rs
pub use context::get_prompt_context;
