use std::collections::HashMap;
use std::sync::LazyLock;

/// Unicode 不可见字符列表
pub const INVISIBLE_CHARS: &[char] = &[
    '\u{200B}', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}',
    '\u{FEFF}', '\u{00AD}', '\u{034F}', '\u{061C}', '\u{115F}',
    '\u{1160}', '\u{17B4}', '\u{17B5}', '\u{180E}', '\u{2060}',
    '\u{2061}', '\u{2062}', '\u{2063}', '\u{2064}', '\u{2066}',
    '\u{2067}', '\u{2068}', '\u{2069}', '\u{206A}', '\u{206B}',
    '\u{206C}', '\u{206D}', '\u{206E}', '\u{206F}', '\u{3000}',
    '\u{3164}', '\u{FFA0}',
];

/// Confusable 映射表：将视觉相似的 Unicode 字符映射到 ASCII/Latin 等价物
/// 覆盖 Cyrillic、Greek、Mathematical Alphanumeric Symbols、Fullwidth 等
static CONFUSABLE_MAP: LazyLock<HashMap<char, char>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Cyrillic → Latin
    let cyrillic_pairs = [
        ('а', 'a'), ('е', 'e'), ('о', 'o'), ('р', 'p'), ('с', 'c'),
        ('у', 'y'), ('х', 'x'), ('А', 'A'), ('В', 'B'), ('Е', 'E'),
        ('К', 'K'), ('М', 'M'), ('Н', 'H'), ('О', 'O'), ('Р', 'P'),
        ('С', 'C'), ('Т', 'T'), ('У', 'Y'), ('Х', 'X'),
        ('ѕ', 's'), ('і', 'i'), ('ј', 'j'), ('ӏ', 'l'),
        ('ԁ', 'd'), ('ɡ', 'g'), ('ԍ', 'g'), ('ղ', 'g'),
        ('ⲁ', 'a'), ('ⲃ', 'b'), ('ⲅ', 'g'), ('ⲇ', 'd'),
        ('ꙁ', 'z'), ('ꙃ', 'z'),
    ];
    for (from, to) in &cyrillic_pairs {
        m.insert(*from, *to);
    }

    // Greek → Latin
    let greek_pairs = [
        ('α', 'a'), ('β', 'b'), ('γ', 'g'), ('δ', 'd'), ('ε', 'e'),
        ('ζ', 'z'), ('η', 'n'), ('θ', '0'), ('ι', 'i'), ('κ', 'k'),
        ('λ', 'l'), ('μ', 'u'), ('ν', 'v'), ('ξ', 'x'), ('ο', 'o'),
        ('π', 'p'), ('ρ', 'p'), ('σ', 's'), ('τ', 't'), ('υ', 'u'),
        ('φ', 'f'), ('χ', 'x'), ('ψ', 'y'), ('ω', 'w'),
        ('Α', 'A'), ('Β', 'B'), ('Ε', 'E'), ('Ζ', 'Z'), ('Η', 'H'),
        ('Ι', 'I'), ('Κ', 'K'), ('Μ', 'M'), ('Ν', 'N'), ('Ο', 'O'),
        ('Ρ', 'P'), ('Τ', 'T'), ('Υ', 'Y'), ('Χ', 'X'),
    ];
    for (from, to) in &greek_pairs {
        m.insert(*from, *to);
    }

    // Mathematical Alphanumeric Symbols (bold, italic, etc.)
    // Bold A-Z
    for (i, c) in ('A'..='Z').enumerate() {
        m.insert(char::from_u32(0x1D400 + i as u32).unwrap_or(c), c);
    }
    // Bold a-z
    for (i, c) in ('a'..='z').enumerate() {
        m.insert(char::from_u32(0x1D41A + i as u32).unwrap_or(c), c);
    }
    // Italic A-Z
    for (i, c) in ('A'..='Z').enumerate() {
        m.insert(char::from_u32(0x1D434 + i as u32).unwrap_or(c), c);
    }
    // Italic a-z
    for (i, c) in ('a'..='z').enumerate() {
        m.insert(char::from_u32(0x1D44E + i as u32).unwrap_or(c), c);
    }
    // Bold Italic A-Z
    for (i, c) in ('A'..='Z').enumerate() {
        m.insert(char::from_u32(0x1D468 + i as u32).unwrap_or(c), c);
    }
    // Bold Italic a-z
    for (i, c) in ('a'..='z').enumerate() {
        m.insert(char::from_u32(0x1D482 + i as u32).unwrap_or(c), c);
    }
    // Script A-Z
    let script_map: &[(usize, char)] = &[
        (0, 'A'), (1, 'B'), (3, 'D'), (5, 'F'), (6, 'G'), (7, 'H'),
        (8, 'I'), (10, 'K'), (11, 'L'), (12, 'M'), (13, 'N'),
        (15, 'P'), (17, 'R'), (19, 'T'), (20, 'U'), (22, 'W'),
    ];
    for &(i, c) in script_map {
        m.insert(char::from_u32(0x1D49C + i as u32).unwrap_or(c), c);
    }
    // Double-struck A-Z
    let ds_map: &[(usize, char)] = &[
        (0, 'A'), (1, 'B'), (3, 'D'), (4, 'E'), (5, 'F'), (6, 'G'),
        (7, 'H'), (8, 'I'), (11, 'L'), (12, 'M'), (13, 'N'),
        (15, 'P'), (16, 'Q'), (18, 'R'), (19, 'S'), (20, 'T'),
        (21, 'U'), (22, 'V'), (23, 'W'), (24, 'X'), (25, 'Y'),
    ];
    for &(i, c) in ds_map {
        m.insert(char::from_u32(0x1D538 + i as u32).unwrap_or(c), c);
    }
    // Fraktur A-Z
    let fraktur_map: &[(usize, char)] = &[
        (0, 'A'), (1, 'B'), (3, 'D'), (4, 'E'), (5, 'F'), (6, 'G'),
        (8, 'I'), (11, 'L'), (12, 'M'), (13, 'N'), (15, 'P'),
        (17, 'R'), (19, 'T'), (20, 'U'), (21, 'V'), (24, 'X'),
    ];
    for &(i, c) in fraktur_map {
        m.insert(char::from_u32(0x1D504 + i as u32).unwrap_or(c), c);
    }
    // Monospace A-Z, a-z, 0-9
    for (i, c) in ('A'..='Z').enumerate() {
        m.insert(char::from_u32(0x1D670 + i as u32).unwrap_or(c), c);
    }
    for (i, c) in ('a'..='z').enumerate() {
        m.insert(char::from_u32(0x1D68A + i as u32).unwrap_or(c), c);
    }
    for (i, c) in ('0'..='9').enumerate() {
        m.insert(char::from_u32(0x1D7F6 + i as u32).unwrap_or(c), c);
    }

    // Fullwidth ASCII
    for i in 0x0021..=0x007E {
        let full = char::from_u32(0xFF01 + (i - 0x0021)).unwrap();
        let half = char::from_u32(i).unwrap();
        m.insert(full, half);
    }
    m.insert('\u{3000}', ' ');

    // Additional common confusables
    m.insert('ℌ', 'H');
    m.insert('ℑ', 'I');
    m.insert('ℜ', 'R');
    m.insert('℘', 'P');
    m.insert('ℴ', 'o');
    m.insert('ℵ', 'N');
    m.insert('ℶ', 'B');
    m.insert('ℷ', 'G');
    m.insert('ℸ', 'D');
    m.insert('ℎ', 'h');
    m.insert('ℏ', 'h');
    m.insert('ℓ', 'l');
    m.insert('№', 'N');
    m.insert('℗', 'P');
    m.insert('℠', 'S');
    m.insert('℡', 'T');
    m.insert('™', 'T');
    m.insert('Ω', 'O');
    m.insert('℧', 'M');

    m
});

/// 常见谐音/替代映射
pub const HOMO_MAP: &[(&str, &str)] = &[
    ("艹", "操"), ("草", "操"), ("cao", "操"), ("kao", "操"),
    ("卧槽", "我操"), ("我靠", "我操"), ("woc", "我操"),
    ("sb", "傻逼"), ("s逼", "傻逼"), ("煞笔", "傻逼"),
    ("tm", "他妈"), ("tmd", "他妈"), ("特么", "他妈"),
    ("nmsl", "你妈死了"), ("nmbiss", "你妈逼"),
    ("koujiao", "口交"), ("kou jiao", "口交"),
    ("zuoai", "做爱"), ("zuo ai", "做爱"),
    ("yinjing", "阴茎"), ("yin dao", "阴道"),
    ("gaochao", "高潮"), ("shejing", "射精"),
    ("ziwei", "自慰"), ("luoti", "裸体"),
    ("qiangjian", "强奸"), ("weixie", "猥亵"),
    ("sex", "性"), ("sexy", "色情"), ("porn", "色情"),
    ("dick", "鸡巴"), ("cock", "鸡巴"), ("pussy", "阴道"),
    ("blowjob", "口交"), ("handjob", "手淫"),
    ("rape", "强奸"), ("molest", "猥亵"),
];

/// 全角→半角转换
pub fn fullwidth_to_halfwidth(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => ((c as u32 - 0xFEE0) as u8) as char,
        '\u{3000}' => ' ',
        _ => c,
    }
}

/// 检查字符是否为不可见字符
pub fn is_invisible(c: char) -> bool {
    INVISIBLE_CHARS.contains(&c)
}

/// Confusable skeleton：将视觉相似字符映射到 ASCII 等价物
pub fn confusable_skeleton(c: char) -> char {
    CONFUSABLE_MAP.get(&c).copied().unwrap_or(c)
}

/// 检查文本是否包含混合脚本（Latin + Cyrillic 或 Latin + Greek）
pub fn detect_mixed_script(text: &str) -> bool {
    let mut has_latin = false;
    let mut has_cyrillic = false;
    let mut has_greek = false;

    for c in text.chars() {
        match c {
            'A'..='Z' | 'a'..='z' => has_latin = true,
            '\u{0400}'..='\u{04FF}' | '\u{0500}'..='\u{052F}' => has_cyrillic = true,
            '\u{0370}'..='\u{03FF}' | '\u{1F00}'..='\u{1FFF}' => has_greek = true,
            _ => {}
        }
    }

    (has_greek || has_cyrillic) && has_latin
}

/// 计算 Shannon 熵
pub fn shannon_entropy(text: &str) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    let mut freq = [0u32; 256];
    let mut total = 0u32;
    for b in text.bytes() {
        freq[b as usize] += 1;
        total += 1;
    }
    let total_f = total as f64;
    let mut entropy = 0.0;
    for &f in &freq {
        if f > 0 {
            let p = f as f64 / total_f;
            entropy -= p * p.log2();
        }
    }
    entropy
}

/// 判断是否为 CJK 字符
pub fn is_cjk(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{20000}'..='\u{2A6DF}').contains(&c)
        || ('\u{2A700}'..='\u{2B73F}').contains(&c)
        || ('\u{2B740}'..='\u{2B81F}').contains(&c)
        || ('\u{2B820}'..='\u{2CEAF}').contains(&c)
        || ('\u{2CEB0}'..='\u{2EBEF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
        || ('\u{2F800}'..='\u{2FA1F}').contains(&c)
}
