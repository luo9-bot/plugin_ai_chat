//! 错别字生成器
//!
//! 基于拼音和字频的中文错别字生成，让 bot 的回复更像真人打字。


use std::collections::HashMap;
use std::sync::OnceLock;
use tracing::debug;

/// 字频数据
static CHAR_FREQUENCY: OnceLock<HashMap<char, f64>> = OnceLock::new();

/// 拼音字典 (拼音 -> 同音字列表)
static PINYIN_DICT: OnceLock<HashMap<String, Vec<char>>> = OnceLock::new();

/// 错别字配置
#[derive(Debug, Clone)]
pub struct TypoConfig {
    /// 单字替换概率
    pub error_rate: f64,
    /// 声调错误概率
    pub tone_error_rate: f64,
    /// 最小字频阈值
    pub min_freq: f64,
    /// 最大频率差
    pub max_freq_diff: f64,
}

impl Default for TypoConfig {
    fn default() -> Self {
        Self {
            error_rate: 0.3,
            tone_error_rate: 0.2,
            min_freq: 5.0,
            max_freq_diff: 200.0,
        }
    }
}

/// 初始化字频数据和拼音字典
pub fn init(data_dir: &std::path::Path) {
    // 加载字频数据
    let freq_path = data_dir.join("char_frequency.json");
    if freq_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&freq_path) {
            if let Ok(raw) = serde_json::from_str::<HashMap<String, f64>>(&content) {
                let freq: HashMap<char, f64> = raw.into_iter()
                    .filter_map(|(k, v)| k.chars().next().map(|c| (c, v)))
                    .collect();
                let _ = CHAR_FREQUENCY.set(freq);
                tracing::info!("typo: loaded char_frequency.json");
            }
        }
    }

    // 加载拼音字典
    let pinyin_path = data_dir.join("pinyin_dict.json");
    if pinyin_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&pinyin_path) {
            if let Ok(raw) = serde_json::from_str::<HashMap<String, Vec<String>>>(&content) {
                let dict: HashMap<String, Vec<char>> = raw.into_iter()
                    .map(|(k, v)| (k, v.into_iter().filter_map(|s| s.chars().next()).collect()))
                    .collect();
                let _ = PINYIN_DICT.set(dict);
                tracing::info!("typo: loaded pinyin_dict.json");
            }
        }
    }
}

/// 应用错别字到文本
pub fn apply_typos(text: &str, config: &TypoConfig) -> String {
    if config.error_rate <= 0.0 {
        return text.to_string();
    }

    let freq = match CHAR_FREQUENCY.get() {
        Some(f) => f,
        None => return text.to_string(),
    };

    let pinyin_dict = match PINYIN_DICT.get() {
        Some(d) => d,
        None => return text.to_string(),
    };

    use rand::Rng;
    let mut rng = rand::thread_rng();

    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());

    for &ch in &chars {
        if is_chinese(ch) && rng.r#gen::<f64>() < config.error_rate {
            if let Some(replacement) = find_homophone(ch, config, freq, pinyin_dict, &mut rng) {
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

/// 找同音字
fn find_homophone(
    ch: char,
    config: &TypoConfig,
    freq: &HashMap<char, f64>,
    pinyin_dict: &HashMap<String, Vec<char>>,
    rng: &mut impl rand::Rng,
) -> Option<char> {
    let pinyin = get_pinyin(ch, pinyin_dict)?;
    let candidates = pinyin_dict.get(&pinyin)?;

    let orig_freq = freq.get(&ch).copied().unwrap_or(0.0);

    // 过滤：排除原字、排除低频字
    let filtered: Vec<(char, f64)> = candidates.iter()
        .filter_map(|c| {
            if *c == ch { return None; }
            let f = freq.get(c).copied().unwrap_or(0.0);
            if f < config.min_freq { return None; }
            Some((*c, f))
        })
        .collect();

    if filtered.is_empty() {
        return None;
    }

    // 按频率相似度加权采样
    let weights: Vec<f64> = filtered.iter()
        .map(|(_, f)| calculate_weight(orig_freq, *f, config.max_freq_diff))
        .collect();

    let total_weight: f64 = weights.iter().sum();
    if total_weight <= 0.0 {
        return None;
    }

    let roll = rng.r#gen::<f64>() * total_weight;
    let mut cumulative = 0.0;
    for (i, &weight) in weights.iter().enumerate() {
        cumulative += weight;
        if roll < cumulative {
            return Some(filtered[i].0);
        }
    }

    filtered.last().map(|(c, _)| *c)
}

/// 计算替换权重（指数衰减）
fn calculate_weight(orig_freq: f64, target_freq: f64, max_freq_diff: f64) -> f64 {
    if target_freq > orig_freq {
        return 1.0;
    }
    let diff = orig_freq - target_freq;
    if diff > max_freq_diff {
        return 0.0;
    }
    (-3.0 * diff / max_freq_diff).exp()
}

/// 获取字符的拼音
fn get_pinyin(ch: char, pinyin_dict: &HashMap<String, Vec<char>>) -> Option<String> {
    // 反向查找：遍历拼音字典找到包含该字符的拼音
    for (py, chars) in pinyin_dict {
        if chars.contains(&ch) {
            return Some(py.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_chinese() {
        assert!(is_chinese('你'));
        assert!(is_chinese('好'));
        assert!(!is_chinese('a'));
        assert!(!is_chinese('1'));
    }

    #[test]
    fn test_calculate_weight() {
        let w = calculate_weight(100.0, 100.0, 200.0);
        assert!((w - 1.0).abs() < 0.01);

        let w = calculate_weight(100.0, 0.0, 200.0);
        assert!(w > 0.0 && w < 1.0);
    }
}
