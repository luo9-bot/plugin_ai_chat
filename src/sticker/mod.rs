//! 洛玖表情包系统
//!
//! 管理表情包的注册、选择和发送。
//! 使用视觉模型（VLM）进行表情包选择和描述生成。

mod manager;
pub mod store;

use luo9_sdk::Msg;
pub use manager::{
    describe_sticker_cq, do_replace_eviction, get_stats, init_ne_stickers, is_sticker_cq,
    maintenance, register_from_cq, steal_emoji_scan,
};

use tracing::info;

/// 发送表情包（供 Planner tool 调用）
///
/// 使用 VLM 子代理从候选网格中选择最合适的表情包，然后通过 SDK 发送。
pub fn send_sticker(
    group_id: u64,
    user_id: u64,
    context_texts: &[String],
    recent_hashes: &[String],
) -> Result<String, String> {
    let context = context_texts.join("\n");

    // 使用 VLM 子代理从候选网格中选择
    let selection = manager::select_sticker_vlm(&context, recent_hashes)
        .ok_or_else(|| "没有可用的表情包，请用文字表达情绪".to_string())?;

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
        return Err("表情包文件缺失，请用文字表达情绪".to_string());
    }

    // 更新使用次数
    manager::update_usage(&selection.hash);

    // 记录到去重追踪器（防重复）
    crate::runtime::reply_dedup::record_sticker(group_id, &selection.hash);

    info!(
        hash = %selection.hash[..16.min(selection.hash.len())],
        description = %selection.description,
        reason = %selection.reason,
        "sticker: sent"
    );

    Ok(selection.description)
}
