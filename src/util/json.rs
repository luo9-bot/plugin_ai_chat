//! JSON 持久化工具

use std::path::Path;

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
