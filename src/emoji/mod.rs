//! Unicode Emoji 处理模块
//!
//! 处理文本中的 Unicode emoji 字符（😍😋😂等），防止污染记忆和学习系统。
//! 注意：图片表情包由 sticker 模块处理。

/// 移除文本中的 Unicode emoji 字符
///
/// 过滤范围：
/// - 表情符号 (U+1F600-U+1F64F)
/// - 杂项符号 (U+1F300-U+1F5FF)
/// - 交通符号 (U+1F680-U+1F6FF)
/// - 补充符号 (U+1F900-U+1F9FF)
/// - 装饰符号 (U+2702-U+27B0)
/// - 零宽连接符 (U+200D)
/// - 变体选择符 (U+FE0F)
pub fn strip_emoji(text: &str) -> String {
    text.chars().filter(|&c| !is_emoji_char(c)).collect()
}

/// 判断字符是否为 Unicode emoji
pub fn is_emoji_char(c: char) -> bool {
    let cp = c as u32;
    matches!(cp,
        0x1F600..=0x1F64F |  // Emoticons
        0x1F300..=0x1F5FF |  // Misc Symbols and Pictographs
        0x1F680..=0x1F6FF |  // Transport and Map
        0x1F900..=0x1F9FF |  // Supplemental Symbols
        0x1FA00..=0x1FA6F |  // Chess Symbols
        0x1FA70..=0x1FAFF |  // Symbols Extended-A
        0x2702..=0x27B0 |    // Dingbats
        0xFE00..=0xFE0F |    // Variation Selectors
        0x200D |              // Zero Width Joiner
        0x20E3 |              // Combining Enclosing Keycap
        0xE0020..=0xE007F |  // Tags
        0x2600..=0x26FF |     // Misc symbols
        0x2700..=0x27BF       // Dingbats
    )
}

/// 检查文本是否几乎全是 emoji（无实际文字内容）
///
/// 去除 emoji 和空白后，如果剩余字符少于 2 个，视为纯 emoji
pub fn is_emoji_only(text: &str) -> bool {
    let stripped = strip_emoji(text);
    let meaningful: String = stripped.chars().filter(|c| !c.is_whitespace()).collect();
    meaningful.len() < 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_emoji() {
        assert_eq!(strip_emoji("hello😍world"), "helloworld");
        assert_eq!(strip_emoji("😋好吃"), "好吃");
        assert_eq!(strip_emoji("没有emoji"), "没有emoji");
        assert_eq!(strip_emoji(""), "");
    }

    #[test]
    fn test_is_emoji_only() {
        assert!(is_emoji_only("😍😋😂"));
        assert!(is_emoji_only("😍 😋"));
        assert!(!is_emoji_only("hello😍"));
        assert!(!is_emoji_only("好吃😋"));
    }

}
