use super::store::{ReplyEffectRecord, FollowupMessage};

// ── 模式列表 ────────────────────────────────────────────────────

/// 负面反馈模式
const NEGATIVE_PATTERNS: &[&str] = &[
    "你没懂", "不是这个意思", "烦死了", "算了", "无语",
    "听不懂", "搞笑", "离谱", "莫名其妙", "你在说什么",
    "搞什么", "脑子有病", "白问了", "浪费时间", "不想理你",
    "你是不是傻", "有毛病", "神经病", "无聊透顶", "废话",
];

/// 修复循环模式
const REPAIR_PATTERNS: &[&str] = &[
    "我是说", "你理解错", "我问的是", "不是这个",
    "再说一次", "你搞错了", "我意思是", "听错了",
    "不对", "重新说", "你听错", "纠正一下",
];

/// 正面反馈模式
const POSITIVE_PATTERNS: &[&str] = &[
    "谢谢", "好的", "明白了", "有用", "不错", "厉害",
    "哈哈", "笑死", "有意思", "可以的", "牛", "绝了",
    "真棒", "太好了", "感谢", "懂了", "知道了", "嗯嗯",
];

// ── ASI 总分 ────────────────────────────────────────────────────

/// ASI = (0.45 * 行为分 + 0.35 * 关系分 + 0.20 * (1 - 摩擦分)) * 100
pub fn calculate_asi(record: &ReplyEffectRecord) -> f64 {
    let behavior = calculate_behavior_score(record);
    let relational = calculate_relational_score(record);
    let friction = calculate_friction_score(record);
    ((0.45 * behavior + 0.35 * relational + 0.20 * (1.0 - friction)) * 100.0).round()
}

// ── 行为分 ──────────────────────────────────────────────────────

fn calculate_behavior_score(record: &ReplyEffectRecord) -> f64 {
    let target: Vec<&FollowupMessage> = record.followups.iter()
        .filter(|f| f.user_id == record.target_user)
        .collect();

    // 是否继续对话（2+ 轮）
    let continue_2turns = if target.len() >= 2 { 1.0 } else if target.len() == 1 { 0.5 } else { 0.0 };

    // 用户情绪估算
    let next_user_sentiment = estimate_sentiment(&target);

    // 用户消息长度（参与度）
    let avg_len: f64 = target.iter().map(|f| f.content.chars().count() as f64).sum::<f64>()
        / target.len().max(1) as f64;
    let user_expansion = ((avg_len - 8.0) / 42.0).clamp(0.0, 1.0);

    // 无纠正
    let no_correction = if target.iter().any(|f| REPAIR_PATTERNS.iter().any(|p| f.content.contains(p))) {
        0.0
    } else {
        1.0
    };

    // 无放弃
    let negative_count = target.iter()
        .filter(|f| NEGATIVE_PATTERNS.iter().any(|p| f.content.contains(p)))
        .count();
    let no_abort = if negative_count >= 2 || target.iter().any(|f| f.content.contains("算了")) {
        0.0
    } else {
        1.0
    };

    (0.30 * continue_2turns + 0.25 * next_user_sentiment + 0.20 * user_expansion
        + 0.15 * no_correction + 0.10 * no_abort)
        .clamp(0.0, 1.0)
}

/// 情绪估算：线性模型
fn estimate_sentiment(messages: &[&FollowupMessage]) -> f64 {
    if messages.is_empty() {
        return 0.5; // 中性
    }
    let mut positive_count = 0.0;
    let mut negative_count = 0.0;
    let mut repair_count = 0.0;

    for msg in messages {
        for p in POSITIVE_PATTERNS {
            if msg.content.contains(p) { positive_count += 1.0; break; }
        }
        for p in NEGATIVE_PATTERNS {
            if msg.content.contains(p) { negative_count += 1.0; break; }
        }
        for p in REPAIR_PATTERNS {
            if msg.content.contains(p) { repair_count += 1.0; break; }
        }
    }

    f64::clamp(0.5 + 0.2 * positive_count - 0.25 * negative_count - 0.15 * repair_count, 0.0, 1.0)
}

// ── 关系分 ──────────────────────────────────────────────────────

/// 关系分：当前使用规则估算，未来可接入 LLM Judge
fn calculate_relational_score(record: &ReplyEffectRecord) -> f64 {
    let target: Vec<&FollowupMessage> = record.followups.iter()
        .filter(|f| f.user_id == record.target_user)
        .collect();

    if target.is_empty() {
        return 0.5; // 无后续消息，中性
    }

    // 社交存在感：用户是否在跟 bot 对话
    let social_presence = if target.len() >= 2 { 0.8 } else { 0.5 };

    // 温暖度：正面情绪比例
    let sentiment = estimate_sentiment(&target);
    let warmth = sentiment;

    // 能力感：无纠正 = 高能力
    let competence = if target.iter().any(|f| REPAIR_PATTERNS.iter().any(|p| f.content.contains(p))) {
        0.3
    } else {
        0.7
    };

    // 恰当性：无负面反馈 = 高恰当性
    let appropriateness = if target.iter().any(|f| NEGATIVE_PATTERNS.iter().any(|p| f.content.contains(p))) {
        0.3
    } else {
        0.7
    };

    (0.35 * social_presence + 0.25 * warmth + 0.25 * competence + 0.15 * appropriateness).clamp(0.0, 1.0)
}

// ── 摩擦分 ──────────────────────────────────────────────────────

fn calculate_friction_score(record: &ReplyEffectRecord) -> f64 {
    // 明确负面反馈（target_user 权重 1.0，其他用户 0.65）
    let explicit_negative = record.followups.iter().map(|f| {
        let weight = if f.user_id == record.target_user { 1.0 } else { 0.65 };
        let max_match = NEGATIVE_PATTERNS.iter()
            .filter(|p| f.content.contains(*p))
            .count() as f64;
        if max_match > 0.0 { weight } else { 0.0 }
    }).fold(0.0f64, f64::max);

    // 修复循环
    let repair_loop = record.followups.iter().map(|f| {
        let weight = if f.user_id == record.target_user { 1.0 } else { 0.65 };
        if REPAIR_PATTERNS.iter().any(|p| f.content.contains(p)) { weight } else { 0.0 }
    }).fold(0.0f64, f64::max);

    // 诡异风险（当前无 LLM Judge，默认 0）
    let uncanny_risk = 0.0;

    (0.40 * explicit_negative + 0.30 * repair_loop + 0.30 * uncanny_risk).clamp(0.0, 1.0)
}

// ── 终结判断 ────────────────────────────────────────────────────

pub fn should_finalize(record: &ReplyEffectRecord) -> bool {
    let now = crate::util::now_secs();

    // 超时终结
    if now.saturating_sub(record.sent_at) >= super::store::OBSERVATION_WINDOW {
        return true;
    }

    // 目标用户 2+ 条后续
    let target_followups = record.followups.iter()
        .filter(|f| f.user_id == record.target_user)
        .count();
    if target_followups >= 2 {
        return true;
    }

    // 总后续消息 5+
    if record.followups.len() >= 5 {
        return true;
    }

    // 明确负面反馈 → 提前终结
    if record.followups.iter().any(|f| {
        f.user_id == record.target_user && NEGATIVE_PATTERNS.iter().any(|p| f.content.contains(p))
    }) {
        return true;
    }

    // 修复循环 → 提前终结
    if record.followups.iter().any(|f| {
        f.user_id == record.target_user && REPAIR_PATTERNS.iter().any(|p| f.content.contains(p))
    }) {
        return true;
    }

    false
}
