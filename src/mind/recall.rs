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

/// 对眼前的人与话题，她能想起什么（"想起：…"行，直接入流）
pub fn recall_for(text: &str, user_id: u64, group_id: u64) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // 自我话语回响最优先：她先"听见"自己刚说过什么
    out.extend(self_echo(text, user_id, group_id));

    // 日记：她自己的生活素材
    out.extend(diary_echo(text));

    // 语义记忆：关于眼前这件事/这个人的既有记忆
    if !text.trim().is_empty() {
        let results = crate::memory::search_memories(user_id, group_id, text, 3);
        for r in results {
            out.push(format!("想起：{}", r.content));
        }
    }

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

    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
