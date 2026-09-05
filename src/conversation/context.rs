//! 场景上下文构建：为语音管线组装"她眼中的世界"
//!
//! 设计原则：
//! - 状态以第一人称体验呈现，不以系统指令呈现。"凌晨两点，眼睛快睁不开了"
//!   而不是"你的电量为 20%，请表演疲惫"——被通知的状态只能被表演，
//!   被体验的状态自然流露。
//! - 记忆围绕"人"组织，按在场者注入，不按调用方便利注入。
//! - 注入量克制：上下文是她的感知，不是一份需要逐条执行的合规清单。

use std::sync::Mutex;

use crate::config;
use crate::emotion::EmotionType;

/// 语音场景描述
pub struct VoiceScene<'a> {
    /// 群号，0 表示私聊
    pub group_id: u64,
    /// 本轮的主要对话者
    pub primary_user_id: u64,
    /// 本轮在场的用户（群聊为发言人去重集合）
    pub involved_users: &'a [u64],
    /// 当前消息文本（用于记忆检索与表达风格匹配）
    pub query_text: &'a str,
    /// 是否有必须回应的理由（被 @ / 危机）
    pub force_reply: bool,
}

/// 向量检索记忆的冷却缓存：每轮对话最多检索一次
static LAST_SEMANTIC_SEARCH: Mutex<Option<std::collections::HashMap<u64, u64>>> = Mutex::new(None);

const SEMANTIC_SEARCH_COOLDOWN: u64 = 120; // 同用户 2 分钟内不重复检索
const SEMANTIC_MEMORY_COUNT: usize = 6;

/// 构建语音管线的系统上下文
pub fn build_voice_context(scene: &VoiceScene) -> String {
    let cfg = config::get();
    let mut parts: Vec<String> = Vec::new();

    parts.push(scene_block(scene));
    if let Some(state) = experience_state_block(scene.primary_user_id) {
        parts.push(state);
    }
    if let Some(people) = people_block(scene) {
        parts.push(people);
    }

    // 她自己的想法（自我记忆）
    let self_mem = crate::self_memory::get_context(cfg.self_reflection.max_thoughts.min(8));
    if !self_mem.is_empty() {
        parts.push(self_mem);
    }

    // 与当前话题相关的长期记忆（带冷却的向量检索）
    if let Some(relevant) =
        relevant_memories_block(scene.primary_user_id, scene.group_id, scene.query_text)
    {
        parts.push(relevant);
    }

    // 对话历史摘要
    let summary = crate::read_shared_state(|s| {
        s.contexts
            .get(&(scene.group_id, scene.primary_user_id))
            .map(|ctx| ctx.conversation_summary.clone())
            .unwrap_or_default()
    });
    if !summary.is_empty() {
        parts.push(format!("# 之前聊过的事（大致记得）\n{}", summary));
    }

    // 日程 / 活动 / 心里的牵挂
    let schedule_ctx = crate::schedule::get_current_context();
    if !schedule_ctx.is_empty() {
        parts.push(schedule_ctx);
    }
    if let Some(activity_ctx) = crate::activity::get_activity_context(scene.primary_user_id) {
        parts.push(activity_ctx);
    }
    let mental_ctx = crate::mental_state::get_prompt_context(
        cfg.mental_state.concerns_max,
        cfg.mental_state.deliberations_max,
    );
    if !mental_ctx.is_empty() {
        parts.push(mental_ctx);
    }

    // 群友的表达习惯（学到的说话方式）
    let expression = crate::learner::get_expression_context(scene.group_id, 5, scene.query_text);
    if !expression.is_empty() {
        parts.push(expression);
    }

    // 自己最近说过的消息（防止复读自己）
    let recent_own =
        crate::read_shared_state(|s| s.get_recent_bot_messages(scene.group_id, 600, 5));
    if !recent_own.is_empty() {
        parts.push(format!(
            "# 你最近说过的话（别重复自己）\n{}",
            recent_own.join("\n")
        ));
    }

    // 可用表情包提示
    let sticker_ctx = crate::sticker::get_sticker_context();
    if !sticker_ctx.is_empty() {
        parts.push(sticker_ctx);
    }

    parts.join("\n\n")
}

// ── 场景块 ──────────────────────────────────────────────────────

fn scene_block(scene: &VoiceScene) -> String {
    let mut text = if scene.group_id == 0 {
        let name = crate::person_info::get_display_name(scene.primary_user_id, 0)
            .unwrap_or_else(|| "对方".to_string());
        format!(
            "# 现在的场景\n你和{name}在一对一私聊，只有你们两个人。\
             这是你们之间的事，不需要@任何人，也不用管群里发生了什么。"
        )
    } else {
        let names: Vec<String> = scene
            .involved_users
            .iter()
            .map(|&uid| {
                crate::person_info::get_display_name(uid, scene.group_id)
                    .unwrap_or_else(|| "群友".to_string())
            })
            .collect();
        format!(
            "# 现在的场景\n你在群 {} 里。这轮说话的人：{}。\
             群里不止你一个人，你的回复所有人都看得到。\
             先看清谁在跟谁说话、有没有人在等你，再决定接不接。",
            scene.group_id,
            names.join("、")
        )
    };

    if scene.force_reply {
        text.push_str("\n这一轮有人直接叫你（或情况特殊），应该给出回应。");
    }
    text
}

// ── 状态 → 第一人称体验 ─────────────────────────────────────────

/// 把当前的电量/节律/情绪/注意力转成她此刻的主观感受
fn experience_state_block(user_id: u64) -> Option<String> {
    let cfg = config::get();
    let mut lines: Vec<String> = Vec::new();

    if cfg.humanity.circadian_enabled
        && let Some(line) = circadian_line(crate::util::current_hour_cst())
    {
        lines.push(line.to_string());
    }

    if cfg.humanity.social_battery_enabled {
        let battery = crate::social_battery::load();
        let frac = battery.level / cfg.humanity.battery_capacity;
        if frac < 0.15 {
            lines.push("今天社交电量见底了，连打字都嫌费劲，谁爱聊谁聊".into());
        } else if frac < 0.3 {
            lines.push("有点累了，不太想多说话".into());
        }
    }

    let emo = crate::emotion::get_state(user_id);
    if let Some(line) = emotion_line(&emo.current, emo.intensity) {
        lines.push(line);
    }

    if cfg.humanity.attention_enabled {
        let attn = crate::conversation::attention::load_attention();
        if attn.attention_level < 0.3 {
            lines.push("注意力有点散，消息都是扫一眼，没在认真想".into());
        }
    }

    if lines.is_empty() {
        return None;
    }
    Some(format!("# 你此刻的状态\n{}", lines.join("\n")))
}

/// 按小时给出一天里的体感
fn circadian_line(hour: u32) -> Option<&'static str> {
    match hour {
        0..=5 => Some("现在是深夜，世界安静得只剩手机屏幕的光，脑子有点木"),
        6..=8 => Some("刚起来没多久，还有点迷糊"),
        9..=11 => Some("上午，精神还行"),
        12..=13 => Some("刚吃过饭，有点犯困"),
        14..=17 => Some("下午，状态不错"),
        18..=22 => Some("晚上，一天里最松弛的时候"),
        23 => Some("夜深了，开始有点困"),
        _ => None,
    }
}

/// 把情绪状态转成她此刻的心情（低强度不提，别没病呻吟）
fn emotion_line(emotion: &EmotionType, intensity: f32) -> Option<String> {
    if intensity < 0.3 {
        return None;
    }
    let text = match emotion {
        EmotionType::Happy => Some("心情不错"),
        EmotionType::Excited => Some("有点兴奋，静不下来"),
        EmotionType::Sad => Some("心里有点闷闷的"),
        EmotionType::Angry => Some("有点火气，说话可能冲"),
        EmotionType::Worried => Some("心里有点惦记事，不太踏实"),
        EmotionType::Tired => Some("累，只想瘫着"),
        EmotionType::Thinking => Some("脑子里在想事情，有点出神"),
        EmotionType::Surprised => None, // 惊讶是瞬时的，不需要注入
        EmotionType::Shy => Some("有点不好意思"),
        _ => None,
    }?;
    Some(format!("此刻的心情：{text}"))
}

// ── 在场的人 ────────────────────────────────────────────────────

/// 每个在场者：怎么称呼、什么关系、记得对方什么
fn people_block(scene: &VoiceScene) -> Option<String> {
    let cfg = config::get();
    let mut blocks: Vec<String> = Vec::new();

    for &uid in scene.involved_users {
        let name = crate::person_info::get_display_name(uid, scene.group_id)
            .unwrap_or_else(|| "群友".to_string());
        let mut lines: Vec<String> = Vec::new();

        if cfg.darling_qq > 0 && uid == cfg.darling_qq {
            lines.push(format!(
                "{name} 是你认定的人。在他面前你不用想那么多，语气自然更软、更有耐心，也更诚实。"
            ));
        }

        let rel = crate::person_info::relationship::get_relationship_context(uid);
        if !rel.is_empty() {
            lines.push(rel);
        }

        let person = crate::person_info::get_person_context(uid);
        if !person.is_empty() {
            lines.push(person);
        }

        let mem = crate::memory::get_context(uid, scene.group_id);
        if !mem.is_empty() {
            lines.push(mem);
        }

        // 危机干预：安全相关，必须保留
        let crisis_level = crate::emotion::get_state(uid).crisis_level;
        let crisis_ctx = crate::emotion::get_crisis_context(crisis_level);
        if !crisis_ctx.is_empty() {
            lines.push(crisis_ctx);
        }

        if lines.is_empty() {
            continue;
        }
        blocks.push(format!("关于{name}：\n{}", lines.join("\n")));
    }

    if blocks.is_empty() {
        return None;
    }
    Some(blocks.join("\n\n"))
}

// ── 相关记忆 ────────────────────────────────────────────────────

fn relevant_memories_block(user_id: u64, group_id: u64, query: &str) -> Option<String> {
    if query.trim().is_empty() {
        return None;
    }

    let now = crate::util::now_secs();
    let should_search = {
        let guard = LAST_SEMANTIC_SEARCH.lock().ok()?;
        guard
            .as_ref()
            .and_then(|m| m.get(&user_id))
            .is_none_or(|&last| now.saturating_sub(last) >= SEMANTIC_SEARCH_COOLDOWN)
    };
    if !should_search {
        return None;
    }

    let relevant = crate::memory::search_memories(user_id, group_id, query, SEMANTIC_MEMORY_COUNT);
    if let Ok(mut guard) = LAST_SEMANTIC_SEARCH.lock() {
        guard
            .get_or_insert_with(std::collections::HashMap::new)
            .insert(user_id, now);
    }
    if relevant.is_empty() {
        return None;
    }

    let lines: Vec<String> = relevant
        .iter()
        .map(|r| format!("- {}", r.content))
        .collect();
    Some(format!(
        "# 想起来的事（跟眼前话题有关）\n{}",
        lines.join("\n")
    ))
}
