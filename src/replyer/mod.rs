//! Replyer 回复生成器
//!
//! 从 Planner 分离出来的回复生成逻辑。
//! 接收 Planner 的参考信息，结合人格/记忆/情绪生成最终回复文本。
//!
//! 支持满足性决策：生成多个候选，第一个超过阈值的直接采用。

use tracing::debug;

/// 回复生成上下文
pub struct ReplyContext {
    pub user_id: u64,
    pub group_id: u64,
    pub user_message: String,
    pub identity: String,
    pub extra_context: String,
    pub history: Vec<(String, String)>,
    pub reference_info: Option<String>,
}

/// 满足性决策引擎
///
/// 模拟人类决策：生成2-3个候选回复，第一个超过阈值的直接采用。
/// 不是选"最好的"，而是选"第一个够好的"——人类不会无限优化一个回复。
pub struct SatisficingEngine {
    /// 满足阈值，受精力/情绪/关系影响变化
    pub threshold: f32,
    /// 最大候选数
    pub max_iterations: u32,
    /// "差不多得了"概率——即使没有超过阈值也直接采用第一个
    pub good_enough_probability: f32,
    /// 当前精力水平（影响threshold）
    pub energy_level: f32,
    /// 当前情绪效价（影响threshold）
    pub emotional_valence: f32,
}

impl Default for SatisficingEngine {
    fn default() -> Self {
        let cfg = crate::config::get();
        let h = &cfg.humanity;
        Self {
            threshold: h.satisficing_threshold,
            max_iterations: h.satisficing_max_iterations,
            good_enough_probability: h.satisficing_good_enough_probability,
            energy_level: 1.0,
            emotional_valence: 0.0,
        }
    }
}

impl SatisficingEngine {
    /// 根据当前状态调整阈值
    ///
    /// 精力越低，阈值越低（更容易满足）
    /// 负面情绪降低阈值（不追求完美回复）
    pub fn adjusted_threshold(&self) -> f32 {
        let energy_mod = 1.0 - (1.0 - self.energy_level) * 0.4;
        let emotion_mod = 1.0 - self.emotional_valence.abs() * 0.2;
        (self.threshold * energy_mod * emotion_mod).clamp(0.1, 0.95)
    }

    /// 判断是否应该直接采用（不经候选生成）
    pub fn should_skip_candidates(&self) -> bool {
        // 低精力时更倾向直接用第一个回复
        let skip_prob = (1.0 - self.energy_level) * 0.6 + 0.05;
        fastrand::f32() < skip_prob
    }

    /// 判断候选回复是否"足够好"
    pub fn is_good_enough(&self, score: f32) -> bool {
        if fastrand::f32() < self.good_enough_probability {
            return true; // "差不多得了"
        }
        score >= self.adjusted_threshold()
    }
}

/// 快速启发式评分（不调用LLM，基于规则）
fn quick_score(reply: &str, ctx: &ReplyContext) -> f32 {
    let mut score = 0.5f32;
    let len = reply.chars().count() as f32;

    // 长度合理（不是太空或太长）
    if len < 3.0 {
        score -= 0.2;
    } else if len > 5.0 && len < 100.0 {
        score += 0.15;
    } else if len > 200.0 {
        score -= 0.1;
    }

    // 包含对用户消息的引用（上下文相关性）
    let user_msg_string: String = ctx.user_message.chars().collect();
    let user_words: Vec<&str> = user_msg_string
        .split(|c: char| !c.is_alphanumeric() && c != '\u{4e00}' && c < '\u{4e00}')
        .filter(|w| w.len() >= 2)
        .collect();
    let mut reference_count = 0;
    for word in &user_words {
        if reply.contains(word) {
            reference_count += 1;
        }
    }
    if reference_count > 0 {
        score += (reference_count as f32 * 0.05).min(0.2);
    }

    // 不是简单的重复
    if ctx.history.iter().any(|(_, content)| content.trim() == reply.trim()) {
        score -= 0.3;
    }

    // 自然的对话长度分布
    score.clamp(0.0, 1.0)
}

/// 生成候选回复（带不同风格/角度）
fn generate_candidate(
    ctx: &ReplyContext,
    variant: u32,
) -> Result<String, String> {
    let cfg = crate::config::get();
    let bot_name = &cfg.bot_name;
    let prompt = crate::prompt::PromptManager::get().raw("replyer");

    let reply_style = select_reply_style(&cfg.style);

    let mut system_parts = vec![prompt.to_string()];

    let identity_block = if ctx.identity.is_empty() {
        String::new()
    } else {
        format!("# 你的身份\n{}", ctx.identity)
    };

    let chat_context = ctx.history.iter()
        .map(|(r, c)| format!("{}: {}", r, c))
        .collect::<Vec<_>>()
        .join("\n");
    let expression_block = crate::learner::get_expression_context(ctx.group_id, 5, &chat_context);

    // 候选变体：注入不同的微调指令
    let variant_hint = match variant {
        0 => "", // 默认风格
        1 => "\n（试着用更简洁的方式表达）",
        2 => "\n（试着用更细腻、更有情感的方式表达）",
        _ => "",
    };

    if !ctx.extra_context.is_empty() {
        system_parts.push(ctx.extra_context.clone());
    }
    if !variant_hint.is_empty() {
        system_parts.push(format!("# 表达微调\n{}", variant_hint.trim()));
    }

    let system_prompt = system_parts.join("\n\n")
        .replace("{bot_name}", bot_name)
        .replace("{identity}", &identity_block)
        .replace("{reply_style}", &reply_style)
        .replace("{expression_block}", &expression_block)
        .replace("{group_chat_attention_block}", "");

    let mut user_content = String::new();
    if !ctx.history.is_empty() {
        user_content.push_str("# 对话历史\n");
        for (role, content) in &ctx.history {
            user_content.push_str(&format!("{}: {}\n", role, content));
        }
        user_content.push('\n');
    }
    if let Some(ref_info) = &ctx.reference_info {
        user_content.push_str(&format!("# 回复参考信息\n{}\n\n", ref_info));
    }
    user_content.push_str(&format!("# 当前消息\n{}", ctx.user_message));

    let (reply, _) = crate::ai::chat(&system_prompt, "", &[], &user_content)?;
    Ok(reply)
}

/// 生成回复文本（带满足性决策）
pub fn generate_reply(ctx: &ReplyContext) -> Result<String, String> {
    let cfg = crate::config::get();

    // 如果不启用满足性决策，走原有单回复路径
    if !cfg.humanity.satisficing_enabled {
        return generate_single_reply(ctx);
    }

    // 构建满足性引擎
    let mut engine = SatisficingEngine::default();

    // 根据注意力调整精力水平
    if cfg.humanity.attention_enabled {
        let attn = crate::conversation::attention::load_attention();
        engine.energy_level = attn.attention_level;
    }

    // 根据情绪调整效价
    let emo = crate::emotion::get_state(ctx.user_id);
    engine.emotional_valence = match emo.current {
        crate::emotion::EmotionType::Happy | crate::emotion::EmotionType::Excited => 0.6,
        crate::emotion::EmotionType::Sad | crate::emotion::EmotionType::Angry => -0.5,
        crate::emotion::EmotionType::Worried => -0.3,
        crate::emotion::EmotionType::Tired => -0.4,
        _ => 0.0,
    };

    // 低精力直接采用首个回复
    if engine.should_skip_candidates() {
        debug!(
            user_id = ctx.user_id,
            energy = engine.energy_level,
            "satisficing: skipping candidates (low energy)"
        );
        return generate_single_reply(ctx);
    }

    let n_candidates = engine.max_iterations.min(3).max(1);

    // 生成第一个候选
    let first = generate_candidate(ctx, 0)?;
    let first_score = quick_score(&first, ctx);

    debug!(
        user_id = ctx.user_id,
        score = first_score,
        threshold = engine.adjusted_threshold(),
        "satisficing: candidate 1"
    );

    // 第一个就足够好，直接采用
    if engine.is_good_enough(first_score) {
        debug!(user_id = ctx.user_id, "satisficing: first candidate accepted");
        return Ok(first);
    }

    // 生成第二个候选
    if n_candidates >= 2 {
        let second = generate_candidate(ctx, 1)?;
        let second_score = quick_score(&second, ctx);

        debug!(
            user_id = ctx.user_id,
            score = second_score,
            "satisficing: candidate 2"
        );

        if engine.is_good_enough(second_score) {
            return Ok(second);
        }

        // 二者择优（但要考虑"差不多得了"）
        if engine.is_good_enough(first_score.max(second_score)) {
            return Ok(if second_score > first_score { second } else { first });
        }
    }

    // 生成第三个候选或直接采用最好的
    if n_candidates >= 3 {
        let third = generate_candidate(ctx, 2)?;
        let third_score = quick_score(&third, ctx);

        debug!(
            user_id = ctx.user_id,
            score = third_score,
            "satisficing: candidate 3"
        );

        // 如果第二个已在上面生成，需要综合比较
        let (best_text, _best_score) = if first_score >= third_score {
            (first, first_score)
        } else {
            (third, third_score)
        };
        return Ok(best_text);
    }

    // 默认返回第一个
    Ok(first)
}

/// 单回复生成（原有逻辑）
fn generate_single_reply(ctx: &ReplyContext) -> Result<String, String> {
    let cfg = crate::config::get();
    let bot_name = &cfg.bot_name;
    let prompt = crate::prompt::PromptManager::get().raw("replyer");

    let reply_style = select_reply_style(&cfg.style);

    let mut system_parts = vec![prompt.to_string()];

    let identity_block = if ctx.identity.is_empty() {
        String::new()
    } else {
        format!("# 你的身份\n{}", ctx.identity)
    };

    let chat_context = ctx.history.iter()
        .map(|(r, c)| format!("{}: {}", r, c))
        .collect::<Vec<_>>()
        .join("\n");
    let expression_block = crate::learner::get_expression_context(ctx.group_id, 5, &chat_context);

    if !ctx.extra_context.is_empty() {
        system_parts.push(ctx.extra_context.clone());
    }

    let system_prompt = system_parts.join("\n\n")
        .replace("{bot_name}", bot_name)
        .replace("{identity}", &identity_block)
        .replace("{reply_style}", &reply_style)
        .replace("{expression_block}", &expression_block)
        .replace("{group_chat_attention_block}", "");

    let mut user_content = String::new();
    if !ctx.history.is_empty() {
        user_content.push_str("# 对话历史\n");
        for (role, content) in &ctx.history {
            user_content.push_str(&format!("{}: {}\n", role, content));
        }
        user_content.push('\n');
    }
    if let Some(ref_info) = &ctx.reference_info {
        user_content.push_str(&format!("# 回复参考信息\n{}\n\n", ref_info));
    }
    user_content.push_str(&format!("# 当前消息\n{}", ctx.user_message));

    debug!(
        user_id = ctx.user_id,
        group_id = ctx.group_id,
        style = %reply_style,
        "replyer: generating"
    );

    let (reply, _) = crate::ai::chat(&system_prompt, "", &[], &user_content)?;
    Ok(reply)
}

/// 选择回复风格（支持随机化）
///
/// 按 `style_random_probability` 概率从 `multiple_reply_styles` 中随机选择，
/// 否则使用默认 `reply_style`。
fn select_reply_style(style_cfg: &crate::config::StyleConfig) -> String {
    if style_cfg.multiple_reply_styles.is_empty() {
        return if style_cfg.reply_style.is_empty() {
            "日常口语化，简短自然".to_string()
        } else {
            style_cfg.reply_style.clone()
        };
    }

    let rand_val: f64 = (crate::util::now_millis() % 1000) as f64 / 1000.0;
    if rand_val < style_cfg.style_random_probability {
        let idx = (crate::util::now_millis() as usize) % style_cfg.multiple_reply_styles.len();
        style_cfg.multiple_reply_styles[idx].clone()
    } else {
        if style_cfg.reply_style.is_empty() {
            "日常口语化，简短自然".to_string()
        } else {
            style_cfg.reply_style.clone()
        }
    }
}
