//! Embedding 向量生成
//!
//! 调用外部 Embedding API（OpenAI 兼容格式）生成文本向量。

use tracing::{debug, warn};

/// Embedding 向量维度（Doubao 模型输出 2560 维）
#[allow(dead_code)]
const EMBEDDING_DIMENSION: usize = 2560;

/// 调用 Embedding API 生成单个文本的向量
pub fn embed_text(text: &str) -> Option<Vec<f32>> {
    let cfg = crate::config::get();
    if !cfg.embedding.enabled() {
        return None;
    }

    let results = embed_batch(&[text.to_string()]);
    results.into_iter().next().flatten()
}

/// 批量生成 embedding（减少 API 调用次数）
pub fn embed_batch(texts: &[String]) -> Vec<Option<Vec<f32>>> {
    let cfg = crate::config::get();
    if !cfg.embedding.enabled() || texts.is_empty() {
        return vec![None; texts.len()];
    }

    let url = format!("{}/embeddings", cfg.embedding.base_url.trim_end_matches('/'));

    let request_body = serde_json::json!({
        "model": cfg.embedding.model,
        "input": texts,
        "encoding_format": "float"
    });

    let json_body = match serde_json::to_string(&request_body) {
        Ok(j) => j,
        Err(e) => {
            warn!(error = %e, "embedding: serialize failed");
            return vec![None; texts.len()];
        }
    };

    debug!(count = texts.len(), model = %cfg.embedding.model, "embedding: sending request");

    let mut resp = match ureq::post(&url)
        .header("Authorization", &format!("Bearer {}", cfg.embedding.api_key))
        .header("Content-Type", "application/json")
        .send(json_body.as_bytes())
    {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "embedding: request failed");
            return vec![None; texts.len()];
        }
    };

    let resp_str = match resp.body_mut().read_to_string() {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "embedding: read response failed");
            return vec![None; texts.len()];
        }
    };

    // 解析响应：OpenAI 兼容格式
    // { "data": [{ "embedding": [0.1, 0.2, ...], "index": 0 }, ...] }
    let parsed: serde_json::Value = match serde_json::from_str(&resp_str) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "embedding: parse response failed");
            return vec![None; texts.len()];
        }
    };

    let data = match parsed.get("data").and_then(|d| d.as_array()) {
        Some(arr) => arr,
        None => {
            warn!("embedding: no 'data' field in response");
            return vec![None; texts.len()];
        }
    };

    // 按 index 排序，构建结果
    let mut results: Vec<Option<Vec<f32>>> = vec![None; texts.len()];
    for item in data {
        let index = item.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        if let Some(embedding) = item.get("embedding").and_then(|e| e.as_array()) {
            let vec: Vec<f32> = embedding
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();
            if index < results.len() {
                results[index] = Some(vec);
            }
        }
    }

    let success_count = results.iter().filter(|r| r.is_some()).count();
    debug!(total = texts.len(), success = success_count, "embedding: completed");

    results
}

/// L2 归一化向量
pub fn l2_normalize(vector: &mut [f32]) {
    let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-10 {
        for x in vector.iter_mut() {
            *x /= norm;
        }
    }
}

/// 计算两个向量的余弦相似度
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| (*x as f64) * (*y as f64)).sum();
    let norm_a: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();

    if norm_a < 1e-10 || norm_b < 1e-10 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}
