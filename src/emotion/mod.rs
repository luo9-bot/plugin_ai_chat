mod state;
mod detect;
mod context;

// ── re-exports ────────────────────────────────────────────────

// state.rs
pub use state::{
    CrisisLevel, EmotionType, EmotionState,
    user_count, get_state, update_state, decay, describe,
};

// detect.rs
pub use detect::{
    detect_crisis, detect_crisis_ai, update_crisis, get_crisis_context,
    analyze_user_message, ai_analyze, update_from_analysis, parse_from_reply,
};

// context.rs
pub use context::get_prompt_context;

// ── 测试 ─────────────────────────────────────────────────────────
// 测试代码已移至 tests/emotion_test.rs
