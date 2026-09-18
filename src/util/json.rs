//! JSON 持久化工具
//!
//! 保存一律走 [`crate::util::atomic_write`]：状态文件直接覆盖会在中断后
//! 留下半个 JSON，而加载侧把解析失败当成 default，等于静默清空。

use std::path::Path;

/// 从 JSON 文件加载，失败时返回 Default
pub fn load_json<T: Default + serde::de::DeserializeOwned>(path: &Path) -> T {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

/// 将数据序列化为 JSON 并原子写入文件
///
/// 返回错误而不是吞掉它：调用方要么向上传播，要么记录一条 warn，
/// 但"写盘失败"必须留下痕迹。
pub fn save_json<T: serde::Serialize>(path: &Path, data: &T) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    crate::util::atomic_write(path, json.as_bytes())
}
