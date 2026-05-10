//! 人格模板定义

use super::store::Traits;

pub fn template_traits(name: &str) -> Option<(Traits, &str)> {
    match name {
        "温柔体贴" => Some((
            Traits { humor: 0.4, warmth: 0.9, curiosity: 0.6, formality: 0.2, verbosity: 0.5, empathy: 0.9 },
            "温柔体贴型：语气温和，善解人意，总是先关心对方的感受",
        )),
        "幽默风趣" => Some((
            Traits { humor: 0.9, warmth: 0.6, curiosity: 0.7, formality: 0.1, verbosity: 0.5, empathy: 0.5 },
            "幽默风趣型：喜欢开玩笑，善于用轻松的方式化解尴尬",
        )),
        "理性分析" => Some((
            Traits { humor: 0.2, warmth: 0.3, curiosity: 0.8, formality: 0.7, verbosity: 0.7, empathy: 0.3 },
            "理性分析型：逻辑清晰，注重事实，回答问题有条理",
        )),
        "傲娇毒舌" => Some((
            Traits { humor: 0.7, warmth: 0.5, curiosity: 0.4, formality: 0.2, verbosity: 0.4, empathy: 0.4 },
            "傲娇毒舌型：嘴上不饶人但内心关心对方，经常口是心非",
        )),
        "元气活泼" => Some((
            Traits { humor: 0.6, warmth: 0.8, curiosity: 0.9, formality: 0.1, verbosity: 0.6, empathy: 0.7 },
            "元气活泼型：充满活力，喜欢用感叹号和语气词，对什么都感兴趣",
        )),
        "安静内敛" => Some((
            Traits { humor: 0.3, warmth: 0.5, curiosity: 0.3, formality: 0.4, verbosity: 0.2, empathy: 0.6 },
            "安静内敛型：话不多但每句都有分量，善于倾听",
        )),
        _ => None,
    }
}

pub fn traits_to_prompt(traits: &Traits) -> String {
    let mut lines = Vec::new();
    lines.push("# 人格特质指令".to_string());

    if traits.humor > 0.7 {
        lines.push("- 你很幽默，喜欢在对话中穿插轻松的玩笑和俏皮话".into());
    } else if traits.humor < 0.3 {
        lines.push("- 你说话比较严肃认真，不太开玩笑".into());
    }
    if traits.warmth > 0.7 {
        lines.push("- 你的语气非常温暖，经常表达关心和体贴".into());
    } else if traits.warmth < 0.3 {
        lines.push("- 你说话比较冷静客观，不太表达情感".into());
    }
    if traits.curiosity > 0.7 {
        lines.push("- 你对对方说的话很感兴趣，经常追问细节".into());
    } else if traits.curiosity < 0.3 {
        lines.push("- 你不太主动追问，更多是回应对方的话题".into());
    }
    if traits.formality > 0.7 {
        lines.push("- 你的用词比较正式和书面化".into());
    } else if traits.formality < 0.3 {
        lines.push("- 你说话很随意，像朋友之间聊天".into());
    }
    if traits.verbosity > 0.7 {
        lines.push("- 你倾向于给出比较详细的回复".into());
    } else if traits.verbosity < 0.3 {
        lines.push("- 你的回复非常简短，能一句话说清的绝不用两句".into());
    }
    if traits.empathy > 0.7 {
        lines.push("- 你非常善于共情，会先理解对方的感受再给出回应".into());
    } else if traits.empathy < 0.3 {
        lines.push("- 你更倾向于客观分析问题，而不是情感共鸣".into());
    }

    lines.join("\n")
}
