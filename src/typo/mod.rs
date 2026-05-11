//! 错别字生成器
//!
//! 让 bot 的回复更像真人打字，偶尔出现同音字错误。


use tracing::debug;

/// 错别字配置
pub struct TypoConfig {
    /// 错误率 (0.0-1.0)
    pub error_rate: f64,
    /// 同音不同调概率
    pub tone_error_rate: f64,
    /// 最小字频阈值
    pub min_freq: u32,
    /// 最大频率差
    pub max_freq_diff: u32,
}

impl Default for TypoConfig {
    fn default() -> Self {
        Self {
            error_rate: 0.3,
            tone_error_rate: 0.2,
            min_freq: 5,
            max_freq_diff: 200,
        }
    }
}

/// 应用错别字到文本
///
/// 对每个中文字符，按概率尝试替换为同音字。
pub fn apply_typos(text: &str, config: &TypoConfig) -> String {
    if config.error_rate <= 0.0 {
        return text.to_string();
    }

    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());

    for (i, &ch) in chars.iter().enumerate() {
        if is_chinese(ch) && pseudo_random(i) < config.error_rate {
            if let Some(replacement) = find_homophone(ch, config) {
                debug!(original = %ch, replacement = %replacement, "typo: applied");
                result.push(replacement);
                continue;
            }
        }
        result.push(ch);
    }

    result
}

/// 判断是否是中文字符
fn is_chinese(ch: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&ch)
}

/// 简单的伪随机数生成（基于索引）
fn pseudo_random(seed: usize) -> f64 {
    let hash = (seed as u64).wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (hash % 1000) as f64 / 1000.0
}

/// 找同音字（简化版：使用静态拼音映射）
fn find_homophone(ch: char, config: &TypoConfig) -> Option<char> {
    let pinyin = get_pinyin(ch)?;
    let candidates = get_pinyin_group(&pinyin)?;

    // 过滤：排除原字、排除低频字
    let filtered: Vec<(char, u32)> = candidates.iter()
        .filter(|(c, freq)| *c != ch && *freq >= config.min_freq)
        .cloned()
        .collect();

    if filtered.is_empty() {
        return None;
    }

    // 按频率相似度加权采样
    let orig_freq = get_char_freq(ch);
    let weights: Vec<f64> = filtered.iter()
        .map(|(_, freq)| {
            let diff = (*freq as f64 - orig_freq as f64).abs();
            (-3.0 * diff / config.max_freq_diff as f64).exp()
        })
        .collect();

    let total_weight: f64 = weights.iter().sum();
    if total_weight <= 0.0 {
        return None;
    }

    let roll = pseudo_random(ch as usize) * total_weight;
    let mut cumulative = 0.0;
    for (i, &weight) in weights.iter().enumerate() {
        cumulative += weight;
        if roll < cumulative {
            return Some(filtered[i].0);
        }
    }

    filtered.last().map(|(c, _)| *c)
}

/// 获取字符的拼音（简化版：返回首字母作为分组键）
fn get_pinyin(ch: char) -> Option<String> {
    // 简化实现：使用 Unicode 范围估算
    // 实际应该使用完整的拼音字典
    let code = ch as u32;
    if code >= 0x4E00 && code <= 0x9FFF {
        // CJK 统一汉字：使用字符编码作为拼音的近似
        Some(format!("py_{}", code % 400))
    } else {
        None
    }
}

/// 获取同拼音组的字符（简化版：返回空）
fn get_pinyin_group(_pinyin: &str) -> Option<Vec<(char, u32)>> {
    // 简化实现：返回空列表
    // 实际应该从拼音字典中查询
    None
}

/// 获取字符频率（简化版：返回默认值）
fn get_char_freq(_ch: char) -> u32 {
    100 // 默认频率
}
