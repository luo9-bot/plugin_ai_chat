//! 文本比较和解析工具

use std::ffi::CString;

/// 解析管理员命令的 QQ 号参数
pub fn parse_uid_arg(msg: &str, prefix: &str) -> Option<Result<u64, String>> {
    let rest = msg.strip_prefix(prefix)?;
    match rest.trim().parse::<u64>() {
        Ok(uid) => Some(Ok(uid)),
        Err(_) => Some(Err(format!("格式: {}QQ号", prefix))),
    }
}

/// 把文本转成传给 SDK 的 C 字符串，**剥离**内嵌 NUL 字节
///
/// `CString::new` 在文本含 NUL 时返回 `Err`，而这里的文本来自模型输出与
/// 用户消息——一个 NUL 就足以让发送路径 panic。NUL 在 QQ 消息里没有任何
/// 意义，因此剥离而不是拒绝发送：一个控制字符不该让整条回复丢掉。
///
/// 返回 `CString` 而不是 `Result`：调用方没有"处理发送失败"的余地，
/// 而这里已经保证不会失败。
pub fn to_c_string(text: impl AsRef<str>) -> CString {
    let text = text.as_ref();
    match CString::new(text) {
        Ok(message) => message,
        Err(error) => {
            let cleaned: Vec<u8> = error
                .into_vec()
                .into_iter()
                .filter(|byte| *byte != 0)
                .collect();
            tracing::warn!("text: 消息含内嵌 NUL，已剥离后发送");
            // 过滤掉 NUL 之后不可能再失败；真失败时退化成空串而不是 panic
            CString::new(cleaned).unwrap_or_default()
        }
    }
}

/// 剥掉 CQ 码，只留正文
///
/// 纯文本层用；含富文本（markdown）与 @ 的消息走
/// [`crate::conversation::perception::normalize`]，那里会把 @ 还原成人名。
/// 这里把各种 CQ 码（图片/视频/转发/at）一律剥掉——它们带签名 URL 与
/// 几百字符的噪声，留在文本里会污染话题匹配与向量检索。
///
/// 放在 `util` 而不是 `conversation`：它是纯字符串函数，而检索层也需要它
/// （查询向量的缓存键必须先去掉 CQ 噪声，否则同一条消息的两种形态
/// 会各占一个缓存项）。
pub fn strip_cq_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[CQ:") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match after.find(']') {
            Some(end) => rest = &after[end + 1..],
            // 没有闭合括号：整段丢弃，避免把半截 CQ 码当正文
            None => return out,
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// 把一段文本规范成**缓存键**形态
///
/// 目的：同一件事的不同写法应当命中同一个缓存项——去掉 CQ 码、折叠空白、
/// 转小写。它只用于缓存键，**不能**用于真正参与匹配的文本（那会改变语义）。
pub fn normalize_cache_key(text: &str) -> String {
    strip_cq_codes(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_passes_through() {
        let message = to_c_string("你好，世界");
        assert_eq!(message.to_str().expect("应为合法 UTF-8"), "你好，世界");
    }

    /// 内嵌 NUL 不能 panic：这是外部输入能触发的真实崩溃点
    #[test]
    fn embedded_nul_is_stripped_instead_of_panicking() {
        let message = to_c_string("前\x00后");
        assert_eq!(
            message.to_str().expect("应为合法 UTF-8"),
            "前后",
            "NUL 必须被剥离，其余内容保留"
        );
    }

    #[test]
    fn only_nul_becomes_empty() {
        assert_eq!(to_c_string("\x00").to_str().expect("合法"), "");
    }

    /// 缓存键必须把"同一件事的不同写法"折叠到一起
    ///
    /// 注意它**折叠**空白而不是删除空白：删掉 `猫粮 买哪种` 里的空格会改变
    /// 分词边界，那就不是"键规范化"而是"改语义"了。
    #[test]
    fn cache_key_collapses_equivalent_queries() {
        let canonical = normalize_cache_key("猫粮买哪种");
        for variant in [
            "猫粮买哪种 ",
            "  猫粮买哪种",
            "猫粮买哪种\n",
            "[CQ:at,qq=1] 猫粮买哪种",
            "猫粮买哪种[CQ:image,file=a.jpg]",
        ] {
            assert_eq!(
                normalize_cache_key(variant),
                canonical,
                "{variant:?} 应当与规范形态同键"
            );
        }

        // 空白形态之间互相折叠（但不等于无空格形态）
        let spaced = normalize_cache_key("猫粮 买哪种");
        for variant in ["猫粮  买哪种", "猫粮\n买哪种", " 猫粮\t买哪种 "] {
            assert_eq!(normalize_cache_key(variant), spaced, "{variant:?}");
        }
        assert_ne!(spaced, canonical, "折叠空白不等于删除空白");
    }

    #[test]
    fn cache_key_lowercases_ascii_without_touching_chinese() {
        assert_eq!(normalize_cache_key("Rust 怎么装"), "rust 怎么装");
        assert_eq!(normalize_cache_key("猫"), "猫");
    }
}
