//! 公共工具函数：消除跨模块重复代码

use std::collections::HashSet;
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

/// 判断闰年
pub fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// 从 epoch 天数计算 (年, 月, 日)，UTC+8
pub fn epoch_days_to_ymd(mut days: u64) -> (u64, u32, u32) {
    let mut y = 1970u64;
    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        y += 1;
    }
    let leap = is_leap_year(y);
    let md = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u32;
    for &dim in &md {
        if days < dim {
            return (y, month, days as u32 + 1);
        }
        days -= dim;
        month += 1;
    }
    (y, 12, 31)
}

/// 当前 UTC+8 的 (年, 月, 日)
pub fn today_ymd() -> (u64, u32, u32) {
    let secs = now_secs() as i64 + 8 * 3600;
    let days = (secs / 86400) as u64;
    epoch_days_to_ymd(days)
}

/// 当前 UTC+8 日期字符串 "YYYY-MM-DD"
pub fn today_str() -> String {
    let (y, m, d) = today_ymd();
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// 当前 UTC+8 日期字符串 "MM-DD"
pub fn current_date_mm_dd() -> String {
    let (_, m, d) = today_ymd();
    format!("{:02}-{:02}", m, d)
}

/// 当前 UTC+8 年份
pub fn current_year() -> u32 {
    today_ymd().0 as u32
}

/// 当前 UTC+8 格式化时间 "HH:MM:SS (YYYY年M月D日)"
pub fn now_formatted_cst() -> String {
    let secs = now_secs() as i64 + 8 * 3600;
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    let (year, month, day) = epoch_days_to_ymd(days as u64);
    format!("{:02}:{:02}:{:02} ({}年{}月{}日)", hour, minute, second, year, month, day)
}

// ── 文本比较 ────────────────────────────────────────────────────

/// 标准化文本用于内容比较：只保留中文字符和字母数字
pub fn normalize_for_compare(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || (*c >= '\u{4e00}' && *c <= '\u{9fff}'))
        .collect::<String>()
        .to_lowercase()
}

/// 计算两段文本的字符重叠比例 (取较短文本为分母)
pub fn content_overlap(a: &str, b: &str) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (shorter, longer) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let shorter_chars: Vec<char> = shorter.chars().collect();
    let longer_set: HashSet<char> = longer.chars().collect();
    let overlap = shorter_chars.iter().filter(|c| longer_set.contains(c)).count();
    overlap as f32 / shorter_chars.len() as f32
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
