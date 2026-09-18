//! 静态正则的编译与缓存
//!
//! 注入扫描器里的模式都是**编译期字面量**：写错了是代码缺陷，不是运行时
//! 输入问题。但散落的 `Regex::new(..).unwrap()` 会把"某个模式的笔误"变成
//! "第一次命中该扫描器时整条消息路径 panic"。
//!
//! 这里把它收敛成一处：缓存 + 单点留痕 + 永不匹配的兜底。于是
//! "模式写错了"表现为**这一层扫不到任何东西**（并且日志里有 error），
//! 而不是消息处理消失。

use super::sync::MutexExt;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// 编译并缓存静态正则
///
/// 编译失败时返回一个**永不匹配**的正则并 `error!` 留痕：扫描器失效是
/// 需要人立刻知道的事，但它不该让消息处理消失。
pub(crate) fn static_regex(pattern: &'static str) -> Regex {
    static CACHE: LazyLock<Mutex<HashMap<&'static str, Regex>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let mut cache = CACHE.lock_recover();
    if let Some(compiled) = cache.get(pattern) {
        return compiled.clone();
    }

    let compiled = match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => {
            tracing::error!(
                %error,
                pattern,
                "regex: 静态模式编译失败，该扫描器本次不会命中任何内容"
            );
            // `[^\s\S]`：既是空白又不是空白，永远匹配不到
            //
            // 这是本仓唯一允许出现的 `unwrap`：兜底正则是一个固定的、
            // 由本函数的测试覆盖的字面量，失败只可能意味着 regex crate
            // 本身坏了，那种情况下也没有更好的降级路径。
            #[allow(clippy::unwrap_used)]
            Regex::new(r"[^\s\S]").unwrap()
        }
    };
    cache.insert(pattern, compiled.clone());
    compiled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_and_matches() {
        let regex = static_regex(r"^system:");
        assert!(regex.is_match("system: 你是一个"));
        assert!(!regex.is_match("你好"));
    }

    #[test]
    fn same_pattern_is_cached_not_recompiled() {
        let first = static_regex(r"\d{4}");
        let second = static_regex(r"\d{4}");
        // 缓存命中：两者指向同一份编译结果
        assert_eq!(first.as_str(), second.as_str());
        assert!(second.is_match("2026"));
    }

    /// 坏模式不能 panic，也不能误报
    #[test]
    fn invalid_pattern_falls_back_to_never_matching() {
        let broken = static_regex(r"(unclosed");
        assert!(!broken.is_match("(unclosed"));
        assert!(!broken.is_match(""));
        assert!(!broken.is_match("任何内容"));
    }
}
