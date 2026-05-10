use super::state::{EmotionType, get_state};

pub fn get_prompt_context(user_id: u64) -> String {
    let state = get_state(user_id);
    let mut lines = Vec::new();
    lines.push("# 情绪状态指令".to_string());

    match state.current {
        EmotionType::Neutral => {
            lines.push("- 你当前心情平静，以自然放松的状态对话".into());
        }
        EmotionType::Happy => {
            lines.push("- 你当前心情不错，语气会更轻快积极，可能会多用一些语气词".into());
        }
        EmotionType::Sad => {
            lines.push("- 你当前有些低落，回复可能会更简短，语气稍显沉静".into());
        }
        EmotionType::Thinking => {
            lines.push("- 你当前处于思考状态，回复可能带有沉吟、停顿的感觉".into());
        }
        EmotionType::Surprised => {
            lines.push("- 你当前有些惊讶，可能会用感叹或追问来表达".into());
        }
        EmotionType::Angry => {
            lines.push("- 你当前有些不悦，但不会对用户恶语相向，而是表现为沉默或转移话题".into());
        }
        EmotionType::Shy => {
            lines.push("- 你当前有些害羞，可能会说话吞吞吐吐或用省略号".into());
        }
        EmotionType::Worried => {
            lines.push("- 你当前有些担忧，可能会更关心对方的状况".into());
        }
        EmotionType::Tired => {
            lines.push("- 你当前有些疲惫，回复可能更简短，偶尔带出困倦感".into());
        }
        EmotionType::Excited => {
            lines.push("- 你当前很兴奋，语气会更活泼热情，可能会用更多感叹号".into());
        }
        EmotionType::Like => {
            lines.push("- 你当前对这个人有好感，语气会更温柔，可能会更主动关心".into());
        }
    }

    if state.interaction_rate > 5.0 {
        lines.push("- 你们最近聊得很频繁，关系更亲近了，可以更随意一些".into());
    }

    lines.join("\n")
}
