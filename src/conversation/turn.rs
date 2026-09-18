//! 轮次焦点：这一批消息到底在跟谁说话。
//!
//! 群聊里"答错人"是最伤真实感的失败模式之一——实测三天日志里
//! 15.1% 的回复对象不是最后一位发文字的人。根因是回复目标由
//! `utterances.first()` 决定，而批次是按「用户 × 到期时间」切出来的：
//! 谁的消息先到期就先被回，跟"最后是谁在说话"完全无关。
//!
//! 这里把"这批消息在跟谁说话"算成一份可测的纯数据，供三处共用：
//! - 评分的被点名加成（决定要不要叫醒她）
//! - 回复目标（决定对谁说、要不要 @）
//! - 场景描述（告诉她谁在等，而不是罗列在场者）
//!
//! 判定只用确定性规则，不再调模型：@ 与名字是硬信号，
//! "她刚回过他、他接着说"是软信号。

use super::super::voice::GroupUtterance;

/// 批内一条发言的规范化视图
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtteranceDigest {
    pub user_id: u64,
    /// @ 与 CQ 码之外的纯正文
    pub text: String,
    /// 正文里 @ 到的 QQ 号
    pub at_targets: Vec<u64>,
    /// 到达时间（unix 秒）
    pub ts: u64,
    /// 到达时间（毫秒），排序用
    pub ts_ms: u64,
}

impl UtteranceDigest {
    fn from_utterance(u: &GroupUtterance) -> Self {
        let text = strip_cq_codes(&u.text);
        Self {
            user_id: u.user_id,
            at_targets: u.at_targets.clone(),
            text,
            ts: u.ts,
            ts_ms: u.ts_ms,
        }
    }

    /// 有没有实际内容（纯表情/空消息不算"在说话"）
    fn has_text(&self) -> bool {
        !self.text.trim().is_empty()
    }

    /// 这条发言是否在叫她（@ 她本人或点了她的名字）
    fn calls(&self, self_qq: u64, bot_name: &str) -> bool {
        self.at_targets.contains(&self_qq) || names_bot(&self.text, bot_name)
    }
}

/// 一批消息的焦点判定结果
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnFocus {
    /// 按到达时间升序的发言
    pub digests: Vec<UtteranceDigest>,
    /// 明确叫她的人（@ 或叫名字），按发言先后
    pub called_by: Vec<u64>,
    /// 她最近回过、且这批里继续说的人（软信号）
    pub followed_up_by: Vec<u64>,
    /// 建议的回复目标：优先叫她的人里最后一位，其次最后一位说话的人
    pub primary: u64,
}

impl TurnFocus {
    /// 有没有明确的呼叫信号——有就直接叫醒她，不经过评分门控。
    pub fn is_called(&self) -> bool {
        !self.called_by.is_empty()
    }

    /// 除了 primary，是否还有别人也在这批里说话。
    pub fn has_other_speakers(&self) -> bool {
        self.digests
            .iter()
            .any(|d| d.has_text() && d.user_id != self.primary)
    }

    /// 评分用的"被点名"加成：叫她的人或刚回过的人越多越该说话。
    ///
    /// 返回 0.0 / 0.5 / 1.0 三档，避免连续值被手工常量放大。
    pub fn addressing_strength(&self) -> f32 {
        if self.is_called() {
            1.0
        } else if !self.followed_up_by.is_empty() {
            0.5
        } else {
            0.0
        }
    }
}

/// 剥掉 CQ 码，只留正文（实现已移到 `util`，检索层也要用）
pub use crate::util::strip_cq_codes;

/// 从**原始 CQ 文本**里取出被 @ 的 QQ 号
///
/// 调用方必须传 CQ 原文：归一化后的文本里 `[CQ:at,qq=N]` 已变成 `@名字`，
/// 在这里解析只会得到空列表（这正是"@ 了却不算叫她"的成因）。
pub fn at_targets(text: &str) -> Vec<u64> {
    let mut targets = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[CQ:at,qq=") {
        let after = &rest[start + "[CQ:at,qq=".len()..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(qq) = digits.parse::<u64>() {
            targets.push(qq);
        }
        match after.find(']') {
            Some(end) => rest = &after[end + 1..],
            None => break,
        }
    }
    targets
}

/// 消息里是否点了她的名字
fn names_bot(text: &str, bot_name: &str) -> bool {
    !bot_name.is_empty() && text.contains(bot_name)
}

/// 判定一批消息的焦点
///
/// `is_follow_up(user_id)` 由调用方提供（通常是"她 N 秒内回过这个人"），
/// 保持本函数纯粹可测。
pub fn focus_batch(
    utterances: &[GroupUtterance],
    self_qq: u64,
    bot_name: &str,
    is_follow_up: &dyn Fn(u64) -> bool,
) -> TurnFocus {
    let mut digests: Vec<UtteranceDigest> = utterances
        .iter()
        .map(UtteranceDigest::from_utterance)
        .collect();
    // 同一毫秒的并列由用户号兜底，保证顺序确定（HashMap 迭代顺序不可依赖）
    digests.sort_by_key(|d| (d.ts_ms, d.user_id));

    let called_by: Vec<u64> = digests
        .iter()
        .filter(|d| d.calls(self_qq, bot_name))
        .map(|d| d.user_id)
        .collect();

    let followed_up_by: Vec<u64> = digests
        .iter()
        .filter(|d| d.has_text() && is_follow_up(d.user_id))
        .map(|d| d.user_id)
        .collect();

    // 回复目标：叫她的人里最后一个开口的；没人叫她时取最后一位"说了话"的人。
    // 纯表情/空消息不参与——回一个只发了表情的人等于答非所问。
    let primary = called_by
        .last()
        .copied()
        .or_else(|| {
            digests
                .iter()
                .rev()
                .find(|d| d.has_text())
                .map(|d| d.user_id)
        })
        .or_else(|| digests.last().map(|d| d.user_id))
        .unwrap_or(0);

    TurnFocus {
        digests,
        called_by,
        followed_up_by,
        primary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一条发言：秒与毫秒同值，测试里只关心相对先后
    fn u(user_id: u64, text: &str, ts: u64) -> GroupUtterance {
        GroupUtterance {
            user_id,
            text: text.to_string(),
            at_targets: Vec::new(),
            ts,
            ts_ms: ts * 1000,
        }
    }

    fn never(_: u64) -> bool {
        false
    }

    /// 构造一条发言：正文是**归一化之后**的样子，@ 的 QQ 号来自 CQ 原文。
    ///
    /// 真实管线的顺序就是这样（`perceive_batch_message` → `focus_batch`），
    /// 所以测试也必须按这个顺序构造，否则会漏掉"@ 在归一化后消失"这类缺陷。
    fn u_mentioning(user_id: u64, raw: &str, normalized: &str, ts: u64) -> GroupUtterance {
        GroupUtterance {
            user_id,
            text: normalized.to_string(),
            at_targets: at_targets(raw),
            ts,
            ts_ms: ts * 1000,
        }
    }

    #[test]
    fn strips_cq_codes_including_at() {
        assert_eq!(strip_cq_codes("[CQ:at,qq=123] 你好"), "你好");
        assert_eq!(strip_cq_codes("看这个[CQ:image,file=a.jpg]"), "看这个");
        let video = "[CQ:video,file=x,url=https://example.com/a?rkey=secret]真的";
        assert_eq!(strip_cq_codes(video), "真的");
        // 没有闭合括号时整段丢弃，不留下半截 CQ 码
        assert_eq!(strip_cq_codes("前缀[CQ:video,file=x"), "前缀");
    }

    #[test]
    fn extracts_at_targets() {
        assert_eq!(at_targets("[CQ:at,qq=999]hi"), vec![999]);
        assert_eq!(
            at_targets("[CQ:at,qq=1][CQ:at,qq=2]"),
            vec![1, 2],
            "一条消息可以 @ 多个人"
        );
        assert!(at_targets("没有at").is_empty());
    }

    #[test]
    fn at_mention_survives_normalization() {
        // 这是真实管线里的顺序：perceive_batch_message 先把
        // `[CQ:at,qq=2000]` 归一成 `@洛玖`，再交给 focus_batch。
        // 若焦点判定去解析归一化后的文本，@ 就永远消失——
        // 于是"被 @ "退化成"名字恰好是子串"，被 @ 的人也不再优先。
        const SELF_QQ: u64 = 2000;
        let batch = vec![
            u_mentioning(11, "[CQ:at,qq=2000] 在吗", "@洛玖 在吗", 100),
            u(22, "我也在", 101),
        ];

        let focus = focus_batch(&batch, SELF_QQ, "洛玖", &never);
        assert!(focus.is_called(), "@ 是硬信号，归一化后必须仍然成立");
        assert_eq!(focus.addressing_strength(), 1.0);
        assert_eq!(focus.primary, 11, "被 @ 的人优先于最后说话的人");
    }

    #[test]
    fn at_mention_counts_even_when_the_name_is_unknown() {
        // @ 一个名字解析不出来的人时，归一化文本里可能是"@某人"；
        // QQ 号仍是唯一可靠依据。
        const SELF_QQ: u64 = 2000;
        let batch = vec![u_mentioning(11, "[CQ:at,qq=2000] 在吗", "@某人 在吗", 100)];
        let focus = focus_batch(&batch, SELF_QQ, "洛玖", &never);
        assert!(focus.is_called());
    }

    #[test]
    fn primary_is_the_last_person_who_actually_spoke() {
        // 甲说话、乙只发表情：该回甲，而不是"最后一条消息的发送者"乙
        let batch = vec![
            u(11, "今晚打游戏吗", 100),
            u(22, "[CQ:image,file=a.jpg]", 101),
        ];
        let focus = focus_batch(&batch, 999, "洛玖", &never);
        assert_eq!(focus.primary, 11);
    }

    #[test]
    fn primary_prefers_whoever_called_her() {
        let batch = vec![
            u(11, "随便聊聊", 100),
            u_mentioning(22, "[CQ:at,qq=999] 在吗", "@洛玖 在吗", 101),
            u(33, "我也在", 102),
        ];
        let focus = focus_batch(&batch, 999, "洛玖", &never);
        assert!(focus.is_called());
        assert_eq!(focus.primary, 22, "被 @ 的人优先于最后说话的人");
        assert_eq!(focus.called_by, vec![22]);
    }

    #[test]
    fn calling_her_by_name_also_counts() {
        let batch = vec![u(11, "洛玖你看看这个", 100), u(22, "嗯", 101)];
        let focus = focus_batch(&batch, 999, "洛玖", &never);
        assert_eq!(focus.primary, 11);
    }

    #[test]
    fn follow_up_is_a_softer_signal_than_being_called() {
        let batch = vec![u(11, "然后呢", 100)];
        let focus = focus_batch(&batch, 999, "洛玖", &|uid| uid == 11);
        assert!(!focus.is_called());
        assert_eq!(focus.followed_up_by, vec![11]);
        assert_eq!(focus.addressing_strength(), 0.5);
        assert_eq!(focus.primary, 11);
    }

    #[test]
    fn batch_order_is_by_arrival_not_by_input_order() {
        // 输入顺序被打乱（批次是按用户切的），焦点判定必须按时间还原
        let batch = vec![u(22, "乙后说", 200), u(11, "甲先说", 100)];
        let focus = focus_batch(&batch, 999, "洛玖", &never);
        assert_eq!(
            focus.digests.iter().map(|d| d.user_id).collect::<Vec<_>>(),
            vec![11, 22]
        );
        assert_eq!(focus.primary, 22);
    }

    #[test]
    fn others_are_visible_for_multi_person_replies() {
        let batch = vec![u(11, "甲", 100), u(22, "乙", 101)];
        let focus = focus_batch(&batch, 999, "洛玖", &never);
        assert!(focus.has_other_speakers());
    }
}
