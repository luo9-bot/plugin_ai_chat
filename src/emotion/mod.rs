mod context;
mod detect;
mod state;

// ── re-exports ────────────────────────────────────────────────

// state.rs
pub use state::{
    CrisisLevel, EmotionState, EmotionTrigger, EmotionType, TriggerType, decay, describe,
    get_state, update_state, user_count,
};

// detect.rs
pub use detect::{
    ai_analyze, analyze_user_message, detect_crisis, detect_crisis_ai, get_crisis_context,
    update_crisis, update_from_analysis,
};

// context.rs
pub use context::get_prompt_context;

// ── 测试 ─────────────────────────────────────────────────────────
// 测试代码已移至 tests/emotion_test.rs
