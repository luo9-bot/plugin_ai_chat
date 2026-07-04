//! 向量语义检索
//!
//! 设计原则：
//! - 查询向量持久化到磁盘，重启后立即可用
//! - 检索只使用已有的向量，绝不阻塞调用 API
//! - 查询向量通过后台线程异步生成并缓存
//! - 向量不可用时优雅降级（返回空，让上层用 BM25）

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::debug;

/// 向量检索结果
pub struct VectorResult {
    pub id: String,
    pub score: f64,
}

// ── 查询向量缓存（内存 + 磁盘） ────────────────────────────────

static QUERY_CACHE: Mutex<Option<HashMap<String, Vec<f32>>>> = Mutex::new(None);
const MAX_CACHE_SIZE: usize = 512;

fn cache_path() -> PathBuf {
    crate::config::data_dir().join("query_embeddings.bin")
}

/// 从磁盘加载查询向量缓存
fn load_cache_from_disk() -> HashMap<String, Vec<f32>> {
    let path = cache_path();
    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };

    if data.len() < 4 {
        return HashMap::new();
    }

    // 格式: [count: u32] [entries: [key_len: u32][key: bytes][dim: u32][vec: f32[dim]]]
    let mut offset = 0;
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    offset += 4;

    let mut cache = HashMap::with_capacity(count);
    for _ in 0..count {
        if offset + 4 > data.len() { break; }
        let key_len = u32::from_le_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]) as usize;
        offset += 4;

        if offset + key_len > data.len() { break; }
        let key = String::from_utf8_lossy(&data[offset..offset+key_len]).into_owned();
        offset += key_len;

        if offset + 4 > data.len() { break; }
        let dim = u32::from_le_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]) as usize;
        offset += 4;

        if offset + dim * 4 > data.len() { break; }
        let mut vec = Vec::with_capacity(dim);
        for d in 0..dim {
            let byte_offset = offset + d * 4;
            vec.push(f32::from_le_bytes([
                data[byte_offset], data[byte_offset+1],
                data[byte_offset+2], data[byte_offset+3],
            ]));
        }
        offset += dim * 4;
        cache.insert(key, vec);
    }

    debug!(count = cache.len(), "vector: query cache loaded from disk");
    cache
}

/// 保存查询向量缓存到磁盘
fn save_cache_to_disk(cache: &HashMap<String, Vec<f32>>) {
    let path = cache_path();
    let mut buf = Vec::new();

    buf.extend_from_slice(&(cache.len() as u32).to_le_bytes());
    for (key, vec) in cache {
        let key_bytes = key.as_bytes();
        buf.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(key_bytes);
        buf.extend_from_slice(&(vec.len() as u32).to_le_bytes());
        for &v in vec {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }

    std::fs::write(&path, buf).ok();
}

/// 初始化查询向量缓存（从磁盘加载）
pub fn init_query_cache() {
    let cache = load_cache_from_disk();
    let mut guard = QUERY_CACHE.lock().unwrap();
    *guard = Some(cache);
}

/// 获取缓存的查询向量（非阻塞，只读缓存）
pub fn get_cached_query_embedding(query: &str) -> Option<Vec<f32>> {
    let guard = QUERY_CACHE.lock().unwrap();
    guard.as_ref()?.get(query).cloned()
}

/// 缓存查询向量（内存 + 异步写磁盘）
pub fn cache_query_embedding(query: String, embedding: Vec<f32>) {
    let should_save = {
        let mut guard = QUERY_CACHE.lock().unwrap();
        let cache = guard.get_or_insert_with(|| load_cache_from_disk());

        // 容量控制：超过上限时清理旧条目
        if cache.len() >= MAX_CACHE_SIZE {
            let keys: Vec<String> = cache.keys().take(MAX_CACHE_SIZE / 4).cloned().collect();
            for k in keys {
                cache.remove(&k);
            }
        }

        cache.insert(query, embedding);
        cache.len() % 32 == 0 // 每 32 次写入保存一次
    };

    if should_save {
        let guard = QUERY_CACHE.lock().unwrap();
        if let Some(ref cache) = *guard {
            save_cache_to_disk(cache);
        }
    }
}

/// 后台生成查询向量（由调用方在独立线程中执行）
pub fn generate_query_embedding(query: &str) -> Option<Vec<f32>> {
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
        "input": [{ "type": "text", "text": query }],
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

    // 缓存结果（内存 + 磁盘）
    cache_query_embedding(query.to_string(), vec.clone());
    debug!(query_len = query.len(), "vector: query embedding generated and cached");

    Some(vec)
}

/// 向量搜索：计算余弦相似度
pub fn search(
    query: &[f32],
    embeddings: &[(String, Vec<f32>)],
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
