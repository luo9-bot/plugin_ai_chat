//! Replyer 回复生成器
//!
//! 从 Planner 分离出来的回复生成逻辑。
//! 接收 Planner 的参考信息，结合人格/记忆/情绪生成最终回复文本。

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

/// 生成回复文本
pub fn generate_reply(ctx: &ReplyContext) -> Result<String, String> {
    let cfg = crate::config::get();
    let prompt = crate::prompt::PromptManager::get().raw("replyer");

    // 选择回复风格（支持随机化）
    let reply_style = select_reply_style(&cfg.style);

    // 构建系统 prompt
    let mut system_parts = vec![prompt.to_string()];

    let identity_block = if ctx.identity.is_empty() {
        String::new()
    } else {
        format!("# 你的身份\n{}", ctx.identity)
    };

    // 获取表达习惯上下文（带 LLM 选择）
    let chat_context = ctx.history.iter()
        .map(|(r, c)| format!("{}: {}", r, c))
        .collect::<Vec<_>>()
        .join("\n");
    let expression_block = crate::learner::get_expression_context(ctx.group_id, 5, &chat_context);

    // 注入额外上下文（记忆、情绪、人格等）
    if !ctx.extra_context.is_empty() {
        system_parts.push(ctx.extra_context.clone());
    }

    let system_prompt = system_parts.join("\n\n")
        .replace("{identity}", &identity_block)
        .replace("{reply_style}", &reply_style)
        .replace("{expression_block}", &expression_block)
        .replace("{group_chat_attention_block}", "");

    // 构建用户消息
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
    // 如果没有配置备选风格，使用默认
    if style_cfg.multiple_reply_styles.is_empty() {
        return if style_cfg.reply_style.is_empty() {
            "日常口语化，简短自然".to_string()
        } else {
            style_cfg.reply_style.clone()
        };
    }

    // 按概率决定是否使用备选风格
    let rand_val: f64 = (crate::util::now_millis() % 1000) as f64 / 1000.0;
    if rand_val < style_cfg.style_random_probability {
        // 从备选风格中随机选择
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
