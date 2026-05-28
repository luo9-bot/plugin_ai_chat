//! 洛玖表情包系统
//!
//! 管理表情包的注册、选择和发送。
//! 使用视觉模型（VLM）进行表情包选择和描述生成。

pub mod store;
mod manager;

use luo9_sdk::Msg;
pub use store::StickerEntry;
pub use store::{find_entry_by_hash, update_vlm_description};
pub use manager::{StickerSelection, register_sticker, register_from_cq, is_sticker_cq, describe_sticker_cq, select_sticker_vlm, update_usage, get_sticker_path, get_stats, maintenance, init_ne_stickers, steal_emoji_scan, do_replace_eviction};

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
        return Err(format!("表情包文件缺失，请用文字表达情绪"));
    }

    // 更新使用次数
    manager::update_usage(&selection.hash);

    // 记录到去重追踪器（防重复）
    use crate::runtime::reply_dedup::ReplyDedupTracker;
    thread_local! {
        static STICKER_DEDUP: std::cell::RefCell<ReplyDedupTracker> = std::cell::RefCell::new(ReplyDedupTracker::new());
    }
    STICKER_DEDUP.with(|d| d.borrow_mut().record_sticker(group_id, &selection.hash));

    info!(
        hash = %selection.hash[..16.min(selection.hash.len())],
        description = %selection.description,
        reason = %selection.reason,
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
        "# 表情包系统\n你有 {} 个可用的表情包。当语言不够到位、用户要求发表情包、或对话氛围需要时，直接调用 send_sticker 工具。系统会自动选择最合适的表情包。如果发送失败，请用文字表达情绪，不要输出[图片]。",
        registered
    )
}
