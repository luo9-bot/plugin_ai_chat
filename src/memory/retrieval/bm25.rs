//! BM25 关键词检索
//!
//! 使用 char 2-gram 分词 + BM25 评分公式

use std::collections::HashMap;

/// BM25 参数
const K1: f64 = 1.5;
const B: f64 = 0.75;

/// BM25 检索结果
pub struct Bm25Result {
    pub id: String,
    pub score: f64,
    /// 匹配到的 token 数量（预留用于调试和分析）
    pub _matched_tokens: usize,
}

/// BM25 搜索
pub fn search(
    query: &str,
    documents: &[(String, String)], // (id, content)
    top_k: usize,
) -> Vec<Bm25Result> {
    if documents.is_empty() || query.is_empty() {
        return Vec::new();
    }

    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return Vec::new();
    }

    // 计算文档平均长度
    let total_len: usize = documents.iter().map(|(_, c)| tokenize(c).len()).sum();
    let avg_len = total_len as f64 / documents.len() as f64;

    // 计算 IDF
    let n = documents.len() as f64;
    let mut df: HashMap<String, usize> = HashMap::new();
    for (_, content) in documents {
        let tokens: std::collections::HashSet<String> = tokenize(content).into_iter().collect();
        for token in tokens {
            *df.entry(token).or_insert(0) += 1;
        }
    }

    let idf: HashMap<String, f64> = query_tokens
        .iter()
        .map(|token| {
            let d = df.get(token).copied().unwrap_or(0) as f64;
            let idf_val = ((n - d + 0.5) / (d + 0.5) + 1.0).ln();
            (token.clone(), idf_val)
        })
        .collect();

    // 计算每个文档的 BM25 分数
    let mut results: Vec<Bm25Result> = documents
        .iter()
        .map(|(id, content)| {
            let doc_tokens = tokenize(content);
            let doc_len = doc_tokens.len() as f64;

            // 计算每个查询词的 TF
            let mut tf: HashMap<String, usize> = HashMap::new();
            for token in &doc_tokens {
                *tf.entry(token.clone()).or_insert(0) += 1;
            }

            let mut score = 0.0;
            let mut matched = 0;
            for qtoken in &query_tokens {
                let tf_val = tf.get(qtoken).copied().unwrap_or(0) as f64;
                if tf_val > 0.0 {
                    matched += 1;
                    let idf_val = idf.get(qtoken).copied().unwrap_or(0.0);
                    let tf_norm = (tf_val * (K1 + 1.0))
                        / (tf_val + K1 * (1.0 - B + B * doc_len / avg_len));
                    score += idf_val * tf_norm;
                }
            }

            Bm25Result {
                id: id.clone(),
                score,
                _matched_tokens: matched,
            }
        })
        .filter(|r| r.score > 0.0)
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(top_k);
    results
}

/// char 2-gram 分词
fn tokenize(text: &str) -> Vec<String> {
    let compact: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect();

    if compact.chars().count() < 2 {
        return vec![compact];
    }

    let chars: Vec<char> = compact.chars().collect();
    (0..chars.len() - 1)
        .map(|i| format!("{}{}", chars[i], chars[i + 1]))
        .collect()
}
