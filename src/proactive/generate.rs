use tracing::debug;

use crate::config;
use crate::emotion;
use crate::memory;
use crate::self_memory;
use crate::working_memory;

use super::runtime::pseudo_random;

/// 用 AI 生成主动消息，失败返回 None
pub fn ai_generate_message(
    trigger: &str,
    user_id: u64,
    group_id: u64,
    emo: &emotion::EmotionState,
) -> Option<String> {
    let mut ctx = Vec::new();

    ctx.push(format!("# 触发类型\n{}", trigger));

    // 时间
    let hour = crate::util::current_hour_cst();
    let time_desc = if hour < 6 { "深夜" } else if hour < 9 { "早上" } else if hour < 12 { "上午" }
        else if hour < 14 { "中午" } else if hour < 18 { "下午" }
        else if hour < 21 { "晚上" } else { "深夜" };
    ctx.push(format!("# 当前时间\n{}:00 ({})", hour, time_desc));

    // 情绪
    ctx.push(format!("# 情绪状态\n{}, intensity: {}", emo.current.as_str(), emo.intensity));

    // 自我记忆
    let self_thoughts = self_memory::get_context(5);
    if !self_thoughts.is_empty() {
        ctx.push(self_thoughts);
    }

    // 用户记忆
    let mem = memory::get_context(user_id);
    if !mem.is_empty() {
        ctx.push(format!("# 关于用户 user_id:{}\n{}", user_id, mem));
    }

    // 群聊工作记忆
    if group_id > 0 {
        let wm = working_memory::get_context(group_id, 3600);
        if !wm.is_empty() {
            ctx.push(wm);
        }
    }

    // 人设
    let personality = crate::personality::get_prompt_context();
    if !personality.is_empty() {
        ctx.push(personality);
    }

    let user_prompt = config::prompt();
    let full_context = ctx.join("\n\n");

    match crate::ai::analyze_with_tools(
        &format!("{}\n\n{}",user_prompt, crate::prompt::PromptManager::get().raw("proactive_message")),
        &full_context,
        &[crate::ai::proactive_message_tool()],
        Some(serde_json::json!("auto"))) {
        Ok(parsed) => {
            let msg = parsed.get("message").and_then(|v| v.as_str()).unwrap_or("");
            if msg.is_empty() {
                debug!("proactive: AI returned empty message");
                None
            } else {
                debug!(msg = %msg, "proactive: AI generated message");
                Some(msg.to_string())
            }
        }
        Err(e) => {
            debug!(error = %e, "proactive: AI generation failed");
            None
        }
    }
}

/// 情绪驱动的消息: AI 生成，失败时 fallback 到硬编码
pub fn generate_mood_message(user_id: u64, emo: &emotion::EmotionState, group_id: u64) -> String {
    if let Some(msg) = ai_generate_message("mood_impulse", user_id, group_id, emo) {
        return msg;
    }
    fallback_mood_message(user_id, emo)
}

/// 硬编码 fallback (AI 不可用时保底)
pub fn fallback_mood_message(user_id: u64, emo: &emotion::EmotionState) -> String {
    let rand = pseudo_random(user_id.wrapping_add(crate::util::now_secs()));
    let self_thoughts = self_memory::get_context(5);
    let has_recent_thought = !self_thoughts.is_empty();

    match emo.current {
        emotion::EmotionType::Happy | emotion::EmotionType::Excited => {
            let options = ["突然心情好好", "嘿嘿", "今天感觉不错", "有点开心", "哈~"];
            let base = options[(rand * options.len() as f64) as usize % options.len()];
            if has_recent_thought && rand > 0.5 {
                let thought = pick_random_thought(&self_thoughts, rand);
                if !thought.is_empty() { return format!("{}\n{}", base, thought); }
            }
            base.to_string()
        }
        emotion::EmotionType::Sad | emotion::EmotionType::Worried => {
            let options = ["有点emo...", "唉", "不知道为什么有点低落", "在想一些事情"];
            options[(rand * options.len() as f64) as usize % options.len()].to_string()
        }
        emotion::EmotionType::Surprised => {
            let options = ["啊 想起来一件事", "对了", "差点忘了说", "噢对"];
            let base = options[(rand * options.len() as f64) as usize % options.len()];
            if has_recent_thought {
                let thought = pick_random_thought(&self_thoughts, rand);
                if !thought.is_empty() { return format!("{}{}", base, thought); }
            }
            base.to_string()
        }
        emotion::EmotionType::Angry => {
            let options = ["有点烦", "啧", "气"];
            options[(rand * options.len() as f64) as usize % options.len()].to_string()
        }
        _ => {
            let options = ["嗯...", "在想事情", "..."];
            options[(rand * options.len() as f64) as usize % options.len()].to_string()
        }
    }
}

/// 从自我记忆文本中随机挑一条想法（排除 [反思] 类，那是 bot 内心活动不应暴露）
pub fn pick_random_thought(context: &str, rand: f64) -> String {
    let lines: Vec<&str> = context.lines()
        .filter(|l| l.starts_with("- ") && l.len() > 4 && !l.contains("[反思]"))
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    let idx = (rand * lines.len() as f64) as usize % lines.len();
    lines[idx].trim_start_matches("- ").to_string()
}

/// 时间问候 -- AI 生成，失败时 fallback 到硬编码
pub fn generate_greeting(user_id: u64, group_id: u64) -> String {
    let emo = emotion::get_state(user_id);
    if let Some(msg) = ai_generate_message("greeting", user_id, group_id, &emo) {
        return msg;
    }
    fallback_greeting(user_id, group_id, &emo)
}

/// 硬编码 fallback (AI 不可用时保底)
pub fn fallback_greeting(user_id: u64, group_id: u64, emo: &emotion::EmotionState) -> String {
    let hour = crate::util::current_hour_cst();
    let rand = pseudo_random(user_id.wrapping_add(crate::util::now_secs()));

    // 基础时间问候 (多选一)
    let time_options: &[&str] = if hour < 6 {
        &["这么晚还没睡吗？注意休息哦", "还不睡呀", "夜猫子"]
    } else if hour < 9 {
        &["早上好~新的一天开始了", "早", "早安~"]
    } else if hour < 12 {
        &["上午好~今天过得怎么样？", "在干嘛呀", "上午好"]
    } else if hour < 14 {
        &["中午好，吃午饭了吗？", "午饭吃什么", "中午好~"]
    } else if hour < 18 {
        &["下午好~在忙什么呢？", "下午好", "在干嘛呢"]
    } else if hour < 21 {
        &["晚上好~今天过得怎么样？", "晚上好", "今天过得怎样"]
    } else {
        &["晚上好~快到休息时间了呢", "还不休息吗", "晚安预备~"]
    };
    let time_greeting = time_options[(rand * time_options.len() as f64) as usize % time_options.len()];

    // 尝试从记忆和自我记忆里加点个性化内容
    let mem = memory::get_context(user_id);
    let self_thoughts = self_memory::get_context(3);
    let wm = working_memory::get_context(group_id, 3600);

    let mut extra = String::new();

    // 个人记忆触发
    if mem.contains("咖啡") && rand > 0.5 {
        extra = "要不要来杯咖啡？".to_string();
    } else if (mem.contains("学习") || mem.contains("工作")) && rand > 0.4 {
        extra = "最近学习/工作还顺利吗？".to_string();
    } else if mem.contains("游戏") && rand > 0.5 {
        extra = "最近有在玩游戏吗？".to_string();
    }

    // 自我记忆触发 (最近反思了什么，可以自然地带出来)
    if extra.is_empty() && !self_thoughts.is_empty() && rand > 0.6 {
        let thought = pick_random_thought(&self_thoughts, rand);
        if !thought.is_empty() {
            extra = thought;
        }
    }

    // 群聊工作记忆触发 (最近群里的事)
    if extra.is_empty() && !wm.is_empty() && rand > 0.7 {
        let lines: Vec<&str> = wm.lines().filter(|l| !l.starts_with('#') && !l.is_empty()).collect();
        if !lines.is_empty() {
            let idx = (rand * 100.0) as usize % lines.len();
            let recent = lines[idx];
            // 不要太刻意，只在有自然关联时提及
            if recent.contains("？") || recent.contains("?") {
                extra = "对了 刚才那个问题解决了吗".to_string();
            }
        }
    }

    // 情绪尾巴
    let emo_suffix = match emo.current {
        emotion::EmotionType::Happy => " 我今天心情不错~",
        emotion::EmotionType::Thinking => " 我在想些事情...",
        emotion::EmotionType::Tired => " 有点困...",
        _ => "",
    };

    if extra.is_empty() {
        format!("{}{}", time_greeting, emo_suffix)
    } else {
        format!("{}{}\n{}", time_greeting, emo_suffix, extra)
    }
}
