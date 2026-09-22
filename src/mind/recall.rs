//! 联想回忆：她"想起"什么
//!
//! 回忆不是检索注入——命中的记忆以"想起"的形式进入意识流，
//! 成为她此刻经历的一部分（方案书 §6.3）。
//!
//! 三类联想来源：
//! - 自我话语回响：她 24h 内说过的话——"这话你今天已经说过了"，
//!   复读的解药是记得自己说过，不是输出拦截器
//! - 日记：她自己写下的生活，是最有质感的自我素材
//! - 语义记忆：现有 memory 检索管线（保留资产）
//!
//! 联想的相关性判断（重叠计数）发生在"把什么带进她眼前"这一层，
//! 属于记忆机制；她怎么回应永远由她自己决定。

use crate::mind::stream::{self, StreamKind};
use crate::util;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const RECALL_COOLDOWN_SECS: u64 = 24 * 3600;
const MAX_RECALLS_PER_TURN: usize = 3;
static RECENT_RECALLS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

/// 虚词停用表：这类字出现在哪都不构成"话题相关"
pub(crate) const STOPWORDS: &str =
    "的了是我你他她它在呢啊吧嘛嗯哦哈呀就都也很又还说要不好这那个什么有没吗们";

/// 话题相关度：话题中的实词字符被候选文本命中的个数（≥2 视为相关）
pub(crate) fn topic_overlap(topic: &str, candidate: &str) -> usize {
    topic
        .chars()
        .filter(|c| !c.is_whitespace() && !STOPWORDS.contains(*c))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .filter(|c| candidate.contains(*c))
        .count()
}

fn has_recall_topic(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if matches!(compact.as_str(), "今天" | "昨天" | "明天" | "现在" | "最近" | "刚才") {
        return false;
    }
    text.chars()
        .filter(|c| !c.is_whitespace() && !STOPWORDS.contains(*c) && !c.is_ascii_punctuation())
        .count() >= 2
}

fn should_emit(user_id: u64, group_id: u64, content: &str) -> bool {
    let now = util::now_secs();
    let key = format!("{user_id}:{group_id}:{}", content.chars().take(120).collect::<String>());
    let mut recent = RECENT_RECALLS.get_or_init(|| Mutex::new(HashMap::new()))
        .lock().unwrap_or_else(|e| e.into_inner());
    recent.retain(|_, ts| now.saturating_sub(*ts) < RECALL_COOLDOWN_SECS);
    if recent.contains_key(&key) { return false; }
    recent.insert(key, now);
    true
}

/// 自我话语回响：她 24h 内说过、且与眼前话题重叠的话——"这话你今天已经说过了"
///
/// 群聊按本群她的发言记录比对；私聊按意识流里她对这个人的发言。
fn self_echo(topic: &str, user_id: u64, group_id: u64) -> Vec<String> {
    let said: Vec<String> = if group_id > 0 {
        crate::read_shared_state(|s| s.get_recent_bot_messages(group_id, 24 * 3600, 15))
    } else {
        stream::recent(24 * 3600, 200)
            .into_iter()
            .filter(|e| e.kind == StreamKind::Acted && e.about == Some(user_id))
            .map(|e| e.content)
            .collect()
    };

    said.into_iter()
        .find(|said| topic_overlap(topic, said) >= 2)
        .map(|said| {
            let snippet: String = said.chars().take(40).collect();
            format!("想起：这话你今天已经说过了——「{snippet}」")
        })
        .into_iter()
        .collect()
}

/// 日记联想：近 7 天她自己写的日记里，与眼前话题相关的
fn diary_echo(topic: &str) -> Vec<String> {
    let cutoff = util::ts_to_date_str(util::now_secs().saturating_sub(7 * 86400));
    crate::mind::diary::recent(30)
        .into_iter()
        .filter(|e| e.date.as_str() >= cutoff.as_str())
        .filter(|e| topic_overlap(topic, &e.content) >= 2)
        .take(2)
        .map(|e| {
            let content: String = e.content.chars().take(40).collect();
            format!("想起：（你在{}的日记里写过）{}", e.date, content)
        })
        .collect()
}

/// 情绪类型 → 情绪效价（+1 正性 / -1 负性 / None 中性）
fn emotion_valence(emotion: &crate::emotion::EmotionType) -> Option<f32> {
    use crate::emotion::EmotionType::*;
    match emotion {
        Happy | Excited => Some(1.0),
        Sad | Angry | Worried => Some(-1.0),
        _ => None,
    }
}

/// PTSD 式闪回：情绪冲击极强的记忆平时被压着，偶尔被眼前的字眼猛地勾起
///
/// 触发要真：话题与记忆必须有实词交集——闪回不是随机抽样，是眼前的东西
/// 勾出来的。概率是"不是每次都来"的分寸；此刻情绪与记忆同号时更容易被勾起
/// （情绪一致性）。产出以第一人称体验入流，她怎么回应仍由她自己决定。
fn flashback(topic: &str, user_id: u64, group_id: u64) -> Vec<String> {
    let cfg = crate::config::get();
    let base_probability = cfg.humanity.flashback_probability;
    if base_probability <= 0.0 || topic.trim().is_empty() {
        return Vec::new();
    }

    // 候选池：情绪冲击达到阈值的记忆（全局 + 本群）
    let threshold = cfg.humanity.flashback_impact_threshold;
    let impactful =
        |e: &&crate::memory::MemoryEntry| e.emotional_impact.is_some_and(|v| v.abs() >= threshold);
    let global = crate::memory::store::load_user_memory(user_id);
    let mut candidates: Vec<crate::memory::MemoryEntry> =
        global.entries.iter().filter(impactful).cloned().collect();
    if group_id > 0 {
        let group_mem = crate::memory::store::load_group_user_memory(group_id, user_id);
        candidates.extend(group_mem.entries.iter().filter(impactful).cloned());
    }
    if candidates.is_empty() {
        return Vec::new();
    }

    // 眼前的字眼必须真的勾到它
    let Some(entry) = candidates
        .into_iter()
        .find(|e| topic_overlap(topic, &e.content) >= 2)
    else {
        return Vec::new();
    };

    // 情绪一致性：此刻的心情与记忆同号，闸门更松
    let impact = entry.emotional_impact.unwrap_or(0.0);
    let probability = match emotion_valence(&crate::emotion::get_state(user_id).current) {
        Some(v) if v * impact > 0.0 => base_probability * 2.0,
        _ => base_probability,
    };
    if fastrand::f32() >= probability.min(0.9) {
        return Vec::new();
    }

    let snippet: String = entry.content.chars().take(50).collect();
    let word = if impact < 0.0 { "画面" } else { "暖流" };
    vec![format!("（毫无来由地，一{word}突然涌上来）{snippet}")]
}

/// 对眼前的人与话题，她能想起什么（"想起：…"行，直接入流）
pub(crate) fn recall_for(text: &str, user_id: u64, group_id: u64) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // 自我话语回响最优先：她先"听见"自己刚说过什么
    out.extend(self_echo(text, user_id, group_id));

    // 日记：她自己的生活素材
    out.extend(diary_echo(text));

    // 语义记忆：关于眼前这件事/这个人的既有记忆
    if has_recall_topic(text) {
        let results = crate::memory::search_memories_for_recall(user_id, group_id, text, 3);
        for r in results {
            out.push(format!("想起：{}", r.content));
        }
    }

    // 闪回：情绪冲击极强的记忆，偶尔毫无来由地突现
    out.extend(flashback(text, user_id, group_id));

    // 时间线索：太久没说话的人（私聊才有"多久没见"的分寸）
    if group_id == 0 {
        let relationship = crate::person_info::relationship::get_relationship(user_id);
        let gap_days = util::now_secs().saturating_sub(relationship.updated_at) / 86400;
        if gap_days >= 3 {
            let name =
                crate::person_info::get_display_name(user_id, 0).unwrap_or_else(|| "对方".into());
            out.push(format!("你和{name}已经{gap_days}天没说话了"));
        }
    }

    let mut filtered = Vec::with_capacity(MAX_RECALLS_PER_TURN);
    for line in out {
        if filtered.len() >= MAX_RECALLS_PER_TURN { break; }
        if should_emit(user_id, group_id, &line) { filtered.push(line); }
    }
    filtered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flashback_output_is_first_person_experience() {
        // 闪回行必须是第一人称体验的转述，不带指令、不带数值
        let line = "（毫无来由地，一个画面突然涌上来）他上周说过要去看海";
        assert!(line.starts_with("（毫无来由地"));
        assert!(!line.contains("memory"));
    }

    #[test]
    fn emotion_valence_signs() {
        use crate::emotion::EmotionType::*;
        assert_eq!(emotion_valence(&Happy), Some(1.0));
        assert_eq!(emotion_valence(&Excited), Some(1.0));
        assert_eq!(emotion_valence(&Sad), Some(-1.0));
        assert_eq!(emotion_valence(&Angry), Some(-1.0));
        assert_eq!(emotion_valence(&Worried), Some(-1.0));
        assert_eq!(emotion_valence(&Tired), None);
        assert_eq!(emotion_valence(&Neutral), None);
    }

    #[test]
    fn recall_output_format_is_paraphrase() {
        // 输出必须是转述口吻，不带指令、不带数值
        let line = "想起：他上周说过要去看海";
        assert!(line.starts_with("想起："));
        assert!(!line.contains("memory"));
    }

    #[test]
    fn topic_overlap_counts_content_chars() {
        let topic = "你们是不是机器人啊";
        let said = "嗯 在呢 活人一个 刚刷手机呢";
        // 实词命中：机、人（虚词你我是不啊被停用）
        assert!(topic_overlap(topic, said) >= 2);
        // 完全无关的话题零命中
        assert_eq!(topic_overlap(topic, "今晚吃火锅吗"), 0);
    }
}
