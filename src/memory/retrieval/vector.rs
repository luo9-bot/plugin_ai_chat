//! 向量语义检索
//!
//! 使用外部 Embedding API 生成向量
//! 内存中计算余弦相似度

/// 向量检索结果
pub struct VectorResult {
    pub id: String,
    pub score: f64,
}

/// 调用多模态向量化 API 生成查询向量
pub fn embed_query(query: &str) -> Option<Vec<f32>> {
    let cfg = crate::config::get();
    if !cfg.embedding.enabled() {
        return None;
    }

    let url = format!(
        "{}/embeddings/multimodal",
        cfg.embedding.base_url.trim_end_matches('/')
    );

    let request_body = serde_json::json!({
        "model": cfg.embedding.model,
        "input": [
            {
                "type": "text",
                "text": query
            }
        ],
        "encoding_format": "float",
        "dimensions": 2048
    });

    let json_body = serde_json::to_string(&request_body).ok()?;

    let mut resp = ureq::post(&url)
        .header("Authorization", &format!("Bearer {}", cfg.embedding.api_key))
        .header("Content-Type", "application/json")
        .send(json_body.as_bytes())
        .ok()?;

    let resp_str = resp.body_mut().read_to_string().ok()?;
    let v: serde_json::Value = serde_json::from_str(&resp_str).ok()?;

    // 提取 embedding 向量（多模态向量化格式）
    // { "data": { "embedding": [0.1, 0.2, ...], "object": "embedding" }, ... }
    let embedding = v
        .get("data")
        .and_then(|d| d.get("embedding"))
        .and_then(|e| e.as_array())?;

    let vec: Vec<f32> = embedding
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();

    if vec.is_empty() {
        return None;
    }

    Some(vec)
}

/// 向量搜索：计算余弦相似度
pub fn search(
    query: &[f32],
    embeddings: &[(String, Vec<f32>)], // (id, embedding)
    top_k: usize,
) -> Vec<VectorResult> {
    if query.is_empty() || embeddings.is_empty() {
        return Vec::new();
    }

    let query_norm = l2_norm(query);
    if query_norm < 1e-10 {
        return Vec::new();
    }

    let mut results: Vec<VectorResult> = embeddings
        .iter()
        .filter_map(|(id, emb)| {
            let emb_norm = l2_norm(emb);
            if emb_norm < 1e-10 {
                return None;
            }
            let dot: f32 = query.iter().zip(emb.iter()).map(|(a, b)| a * b).sum();
            let cosine = (dot / (query_norm * emb_norm)) as f64;
            Some(VectorResult {
                id: id.clone(),
                score: cosine,
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(top_k);
    results
}

fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}
