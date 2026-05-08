//! 公共工具函数：消除跨模块重复代码

use std::path::Path;
use std::time::SystemTime;

// ── 时间工具 ────────────────────────────────────────────────────

/// 当前 Unix 时间戳（秒）
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 当前 Unix 时间戳（毫秒）
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 当前 UTC+8 小时数 (0-23)
pub fn current_hour_cst() -> u32 {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64 + 8 * 3600;
    ((secs % 86400) / 3600) as u32
}

// ── JSON 持久化 ─────────────────────────────────────────────────

/// 从 JSON 文件加载，失败时返回 Default
pub fn load_json<T: Default + serde::de::DeserializeOwned>(path: &Path) -> T {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

/// 从 JSON 文件加载为 serde_json::Value，失败时返回空对象
pub fn load_json_value(path: &Path) -> serde_json::Value {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    }
}

/// 将数据序列化为 JSON 并写入文件
pub fn save_json<T: serde::Serialize>(path: &Path, data: &T) {
    if let Ok(json) = serde_json::to_string_pretty(data) {
        std::fs::write(path, json).ok();
    }
}

// ── 命令解析 ────────────────────────────────────────────────────

/// 解析管理员命令的 QQ 号参数
///
/// - `msg`: 原始消息
/// - `prefix`: 命令前缀，如 "开启群聊:"
///
/// 返回：
/// - `None` — 消息不匹配前缀
/// - `Some(Ok(uid))` — 成功解析
/// - `Some(Err(format_msg))` — 前缀匹配但解析失败
pub fn parse_uid_arg<'a>(msg: &'a str, prefix: &str) -> Option<Result<u64, String>> {
    let rest = msg.strip_prefix(prefix)?;
    match rest.trim().parse::<u64>() {
        Ok(uid) => Some(Ok(uid)),
        Err(_) => Some(Err(format!("格式: {}QQ号", prefix))),
    }
}
