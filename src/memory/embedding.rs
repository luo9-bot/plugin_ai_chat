//! Embedding 向量生成
//!
//! 调用火山引擎多模态向量化 API 生成文本向量。
//! API 文档: https://www.volcengine.com/docs/82379/1409291

use tracing::{debug, warn};

/// Embedding 向量维度（doubao-embedding-vision 默认输出 2048 维）
const EMBEDDING_DIMENSION: usize = 2048;

/// 单次向量化请求的超时（秒）
///
/// 每个文本是一次独立请求（该 API 不支持批量返回），所以这个值同时决定了
/// 一批 N 条文本的最坏耗时 N × 该值。选 10 秒是因为向量化是后台补数据，
/// 单个文本不值得等更久。
const EMBEDDING_TIMEOUT_SECS: u64 = 10;

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

    // 10 秒超时 + 复用连接池：一批文本会连续发 N 次请求，
    // 每次重新握手在"后台补向量"这个场景下纯属浪费
    let agent = crate::util::agent(crate::util::AgentSpec::requiring_success(
        EMBEDDING_TIMEOUT_SECS,
    ));

    let resp_str = match crate::util::post_json(&agent, &url, &cfg.embedding.api_key, &json_body) {
        Ok(body) => body,
        Err(error) => {
            warn!(error = %error, "embedding: request failed");
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

    debug!(model = %cfg.embedding.model, len = vec.len(), "embedding: completed");

    Some(vec)
}

/// 批量生成 embedding（对每个文本单独调用多模态 API）
///
/// 注意：多模态向量化 API 不支持旧版批量返回格式，
/// 每个 input 数组整体只返回一个向量，因此需要逐个调用。
/// 为避免内存溢出，限制每批最大处理数量。
pub fn embed_batch(texts: &[String]) -> Vec<Option<Vec<f32>>> {
    let cfg = crate::config::get();
    if !cfg.embedding.enabled() || texts.is_empty() {
        return vec![None; texts.len()];
    }

    // 限制批次大小，避免一次性处理过多导致内存溢出
    const MAX_BATCH_SIZE: usize = 50;
    let mut results = Vec::with_capacity(texts.len());

    for chunk in texts.chunks(MAX_BATCH_SIZE) {
        let chunk_results: Vec<Option<Vec<f32>>> = chunk.iter().map(|t| embed_single(t)).collect();
        results.extend(chunk_results);
    }

    let success_count = results.iter().filter(|r| r.is_some()).count();
    if success_count < texts.len() {
        debug!(
            total = texts.len(),
            success = success_count,
            "embedding: batch completed (partial)"
        );
    } else {
        debug!(total = texts.len(), "embedding: batch completed");
    }

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
