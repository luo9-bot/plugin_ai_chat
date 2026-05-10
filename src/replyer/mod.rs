//! Replyer 回复生成器
//!
//! 从 Planner 分离出来的回复生成逻辑。
//! 接收 Planner 的参考信息，结合人格/记忆/情绪生成最终回复文本。
//! 参考 MaiBot 的 replyer 架构。

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
///
/// 使用 replyer prompt 模板，注入人格/记忆/情绪/表达习惯等上下文。
pub fn generate_reply(ctx: &ReplyContext) -> Result<String, String> {
    let cfg = crate::config::get();
    let prompt = crate::prompt::PromptManager::get().raw("replyer");

    // 选择回复风格
    let reply_style = select_reply_style(cfg);

    // 构建系统 prompt
    let mut system_parts = vec![prompt.to_string()];

    // 替换占位符
    let identity_block = if ctx.identity.is_empty() {
        String::new()
    } else {
        format!("# 你的身份\n{}", ctx.identity)
    };

    // 获取表达习惯上下文
    let expression_block = crate::learner::get_expression_context(ctx.group_id, 5);

    // 注入额外上下文（记忆、情绪、人格等）
    if !ctx.extra_context.is_empty() {
        system_parts.push(ctx.extra_context.clone());
    }

    let system_prompt = system_parts.join("\n\n")
        .replace("{identity}", &identity_block)
        .replace("{reply_style}", &reply_style)
        .replace("{expression_block}", &expression_block)
        .replace("{group_chat_attention_block}", "");

    // 构建用户消息（历史 + 当前消息）
    let mut user_content = String::new();
    if !ctx.history.is_empty() {
        user_content.push_str("# 对话历史\n");
        for (role, content) in &ctx.history {
            user_content.push_str(&format!("{}: {}\n", role, content));
        }
        user_content.push('\n');
    }

    // 添加参考信息
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

    // 调用 AI 生成回复
    let (reply, _) = crate::ai::chat(&system_prompt, "", &[], &user_content)?;
    Ok(reply)
}

/// 选择回复风格（支持随机化）
fn select_reply_style(_cfg: &crate::config::Config) -> String {
    // TODO: 实现 multiple_reply_styles 随机选择
    // 目前使用默认风格
    let base_style = "日常口语化，简短自然";
    base_style.to_string()
}
