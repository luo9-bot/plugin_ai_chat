//! 洛玖表情包系统
//!
//! 管理表情包的注册、选择和发送。
//! 使用视觉模型（VLM）进行表情包选择和描述生成。

pub mod store;
mod manager;

use luo9_sdk::Msg;
pub use store::StickerEntry;
pub use manager::{StickerSelection, register_sticker, register_from_cq, select_sticker_vlm, update_usage, get_sticker_path, get_stats, maintenance};

use tracing::info;

/// 发送表情包（供 Planner tool 调用）
///
/// 使用 VLM 从候选中选择最合适的表情包，然后通过 SDK 发送。
pub fn send_sticker(
    group_id: u64,
    user_id: u64,
    target_emotion: &str,
    context: &str,
    recent_hashes: &[String],
) -> Result<String, String> {
    // 使用 VLM 选择最合适的表情包
    let selection = manager::select_sticker_vlm(context, target_emotion, recent_hashes)
        .ok_or_else(|| "没有可用的表情包".to_string())?;

    // 读取图片文件路径
    let data_dir = crate::config::data_dir();
    let full_path = data_dir.join(&selection.path);

    // 使用 SDK 发送图片消息
    if full_path.exists() {
        let path_str = full_path.to_string_lossy().to_string();
        let msg = Msg::image(path_str).build();
        if group_id > 0 {
            luo9_sdk::Bot::send_group_msg(group_id, msg);
        } else {
            luo9_sdk::Bot::send_private_msg(user_id, msg);
        }
    } else {
        // 文件不存在时发送文本描述
        let msg = format!("[表情包: {}]", selection.description);
        crate::sender::send_msg(group_id, user_id, &msg);
    }

    // 更新使用次数
    manager::update_usage(&selection.hash);

    info!(
        hash = %selection.hash[..16.min(selection.hash.len())],
        emotion = target_emotion,
        description = %selection.description,
        "sticker: sent"
    );

    Ok(selection.description)
}

/// 获取表情包上下文（注入到 Planner prompt）
pub fn get_sticker_context() -> String {
    let (_total, registered) = manager::get_stats();
    if registered == 0 {
        return String::new();
    }
    format!(
        "# 表情包系统\n你有 {} 个可用的表情包。当对话需要情感表达时，可以调用 send_sticker 工具发送合适的表情包。",
        registered
    )
}
