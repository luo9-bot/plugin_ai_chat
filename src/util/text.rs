//! 文本比较和解析工具

use std::collections::HashSet;

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

/// 解析管理员命令的 QQ 号参数
pub fn parse_uid_arg(msg: &str, prefix: &str) -> Option<Result<u64, String>> {
    let rest = msg.strip_prefix(prefix)?;
    match rest.trim().parse::<u64>() {
        Ok(uid) => Some(Ok(uid)),
        Err(_) => Some(Err(format!("格式: {}QQ号", prefix))),
    }
}
