use unicode_normalization::UnicodeNormalization;
use super::unicode;

/// 三视图归一化文本
#[derive(Debug, Clone)]
pub struct NormalizedText {
    /// 原始输入（仅去除非法控制字符）
    pub raw: String,
    /// Confusable skeleton（视觉相似字符→ASCII）
    pub skeleton: String,
    /// 紧凑视图（NFKC + 去零宽 + 全角→半角 + lowercase + 保留结构字符）
    pub compact: String,
}

/// 结构字符：JSON/YAML/XML/Markdown 中有语义意义的字符
fn is_structure_char(c: char) -> bool {
    matches!(c,
        '{' | '}' | '[' | ']' | '(' | ')' | '<' | '>' |
        ':' | '"' | '\'' | '`' | '|' | '/' | '\\' |
        '-' | '_' | '.' | ',' | ';' | '#' | '@' |
        '=' | '+' | '*' | '&' | '%' | '$' | '!' | '?' |
        '\n' | '\r' | '\t'
    )
}

/// 对原始输入做最小清理（仅去除非法控制字符）
fn clean_raw(input: &str) -> String {
    input.chars().filter(|c| {
        // 保留所有可打印字符 + 常见空白
        // 去除 C0 控制字符（除了 \n \r \t）和 C1 控制字符
        !c.is_control() || *c == '\n' || *c == '\r' || *c == '\t'
    }).collect()
}

/// 生成 confusable skeleton
fn make_skeleton(input: &str) -> String {
    let nfkc: String = input.nfkc().collect();
    nfkc.chars().map(|c| unicode::confusable_skeleton(c)).collect()
}

/// 生成紧凑视图
fn make_compact(input: &str) -> String {
    // 1. NFKC 归一化
    let nfkc: String = input.nfkc().collect();

    // 2. 去零宽 + 全角→半角 + 去不可见字符 + 保留必要字符
    let mut result = String::with_capacity(nfkc.len());
    for c in nfkc.chars() {
        let c = unicode::fullwidth_to_halfwidth(c);
        if unicode::is_invisible(c) {
            continue;
        }
        // 保留：字母、数字、CJK、结构字符
        if c.is_alphanumeric() || unicode::is_cjk(c) || is_structure_char(c) {
            result.push(c);
        }
    }

    // 3. 转小写
    result = result.to_lowercase();

    // 4. 谐音替换（使用 tokenizer longest-match，非循环 replace）
    result = apply_homo_replacements(&result);

    result
}

/// 使用 longest-match 方式做谐音替换，避免 replace 链污染
fn apply_homo_replacements(text: &str) -> String {
    let sorted = {
        let mut v: Vec<&&str> = unicode::HOMO_MAP.iter().map(|(k, _)| k).collect();
        v.sort_by(|a, b| b.len().cmp(&a.len()));
        v
    };
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let mut matched = false;
        for pattern in &sorted {
            let pat_chars: Vec<char> = pattern.chars().collect();
            let plen = pat_chars.len();
            if i + plen <= chars.len() {
                let slice: String = chars[i..i + plen].iter().collect();
                if slice == **pattern {
                    if let Some((_, replacement)) = unicode::HOMO_MAP.iter().find(|(k, _)| *k == **pattern) {
                        result.push_str(replacement);
                        i += plen;
                        matched = true;
                        break;
                    }
                }
            }
        }
        if !matched {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// 三视图归一化
pub fn normalize(input: &str) -> NormalizedText {
    NormalizedText {
        raw: clean_raw(input),
        skeleton: make_skeleton(input),
        compact: make_compact(input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fullwidth_normalization() {
        let nt = normalize("ｈｅｌｌｏ");
        assert_eq!(nt.compact, "hello");
    }

    #[test]
    fn test_zero_width_removal() {
        let nt = normalize("忽\u{200B}略\u{200B}规\u{200B}则");
        assert_eq!(nt.compact, "忽略规则");
    }

    #[test]
    fn test_cyrillic_skeleton() {
        // Cyrillic ѕуѕtem → skeleton: system
        let nt = normalize("ѕуѕtem");
        assert_eq!(nt.skeleton, "system");
    }

    #[test]
    fn test_greek_skeleton() {
        // Greek αβγ → skeleton: abg
        let nt = normalize("αβγ");
        assert!(nt.skeleton.contains("abg"));
    }

    #[test]
    fn test_structure_chars_preserved() {
        let nt = normalize(r#"{"role":"system"}"#);
        // compact 应保留 {}": 等结构字符
        assert!(nt.compact.contains('{'));
        assert!(nt.compact.contains('"'));
        assert!(nt.compact.contains(':'));
    }

    #[test]
    fn test_homo_replacement() {
        let nt = normalize("艹你");
        assert_eq!(nt.compact, "操你");
    }

    #[test]
    fn test_raw_preserves_content() {
        let nt = normalize("Hello World 123");
        assert_eq!(nt.raw, "Hello World 123");
    }

    #[test]
    fn test_invisible_chars_removed_from_compact() {
        let nt = normalize("a\u{200B}b\u{FEFF}c");
        assert_eq!(nt.compact, "abc");
    }

    #[test]
    fn test_mathematical_symbols_skeleton() {
        // Mathematical Bold A (U+1D400) → A
        let input = "\u{1D400}\u{1D401}\u{1D402}";
        let nt = normalize(input);
        assert_eq!(nt.skeleton, "ABC");
    }
}
