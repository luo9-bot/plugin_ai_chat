//! Embedding 向量生成
//!
//! 调用火山引擎多模态向量化 API 生成文本向量。
//! API 文档: https://www.volcengine.com/docs/82379/1409291

use tracing::{debug, warn};

/// Embedding 向量维度（doubao-embedding-vision 默认输出 2048 维）
#[allow(dead_code)]
const EMBEDDING_DIMENSION: usize = 2048;

/// 调用 Embedding API 生成单个文本的向量
pub fn embed_text(text: &str) -> Option<Vec<f32>> {
    let cfg = crate::config::get();
    if !cfg.embedding.enabled() {
        return None;
    }

    embed_single(text)
}

/// 调用多模态向量化 API，对单个文本生成向量
fn embed_single(text: &str) -> Option<Vec<f32>> {
    let cfg = crate::config::get();

    let url = format!(
        "{}/embeddings/multimodal",
        cfg.embedding.base_url.trim_end_matches('/')
    );

    let request_body = serde_json::json!({
        "model": cfg.embedding.model,
        "input": [
            {
                "type": "text",
                "text": text
            }
        ],
        "encoding_format": "float",
        "dimensions": EMBEDDING_DIMENSION
    });

    let json_body = match serde_json::to_string(&request_body) {
        Ok(j) => j,
        Err(e) => {
            warn!(error = %e, "embedding: serialize failed");
            return None;
        }
    };

    debug!(model = %cfg.embedding.model, "embedding: sending request");

    let mut resp = match ureq::post(&url)
        .header("Authorization", &format!("Bearer {}", cfg.embedding.api_key))
        .header("Content-Type", "application/json")
        .send(json_body.as_bytes())
    {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "embedding: request failed");
            return None;
        }
    };

    let resp_str = match resp.body_mut().read_to_string() {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "embedding: read response failed");
            return None;
        }
    };

    // 解析响应：多模态向量化格式
    // { "data": { "embedding": [0.1, 0.2, ...], "object": "embedding" }, ... }
    let parsed: serde_json::Value = match serde_json::from_str(&resp_str) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "embedding: parse response failed");
            return None;
        }
    };

    let embedding = match parsed
        .get("data")
        .and_then(|d| d.get("embedding"))
        .and_then(|e| e.as_array())
    {
        Some(arr) => arr,
        None => {
            warn!("embedding: no 'data.embedding' field in response");
            return None;
        }
    };

    let vec: Vec<f32> = embedding
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();

    if vec.is_empty() {
        warn!("embedding: received empty embedding vector");
        return None;
    }

    debug!("embedding: completed");
    Some(vec)
}

/// 批量生成 embedding（对每个文本单独调用多模态 API）
///
/// 注意：多模态向量化 API 不支持旧版批量返回格式，
/// 每个 input 数组整体只返回一个向量，因此需要逐个调用。
pub fn embed_batch(texts: &[String]) -> Vec<Option<Vec<f32>>> {
    let cfg = crate::config::get();
    if !cfg.embedding.enabled() || texts.is_empty() {
        return vec![None; texts.len()];
    }

    let results: Vec<Option<Vec<f32>>> = texts
        .iter()
        .map(|t| {
            debug!("embedding: sending batch item");
            embed_single(t)
        })
        .collect();

    let success_count = results.iter().filter(|r| r.is_some()).count();
    debug!(total = texts.len(), success = success_count, "embedding: batch completed");

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
