//! 向量语义检索
//!
//! 设计原则：
//! - 查询向量持久化到磁盘，重启后立即可用
//! - 检索只使用已有的向量，绝不阻塞调用 API
//! - 查询向量通过后台线程异步生成并缓存
//! - 向量不可用时优雅降级（返回空，让上层用 BM25）

use crate::util::MutexExt;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::debug;

/// 向量检索结果
pub struct VectorResult {
    pub id: String,
    pub score: f64,
}

// ── 查询向量缓存（内存 + 磁盘） ────────────────────────────────

/// 缓存：键是**规范化后的查询**，另加一个插入顺序队列用于淘汰
struct QueryCache {
    entries: HashMap<String, Vec<f32>>,
    /// 插入顺序（先进先出）。原先的淘汰是 `keys().take(n)`——HashMap 的
    /// 迭代序是任意的，因此"淘汰四分之一"实际淘汰谁不可预测。
    order: VecDeque<String>,
}

impl QueryCache {
    fn from_entries(entries: HashMap<String, Vec<f32>>) -> Self {
        let order = entries.keys().cloned().collect();
        Self { entries, order }
    }

    fn get(&self, key: &str) -> Option<&Vec<f32>> {
        self.entries.get(key)
    }

    fn insert(&mut self, key: String, embedding: Vec<f32>, capacity: usize) {
        if self.entries.insert(key.clone(), embedding).is_none() {
            self.order.push_back(key);
        }
        // 超容量就按插入顺序淘汰最旧的
        while self.entries.len() > capacity {
            match self.order.pop_front() {
                Some(oldest) => {
                    self.entries.remove(&oldest);
                }
                None => break,
            }
        }
    }
}

static QUERY_CACHE: Mutex<Option<QueryCache>> = Mutex::new(None);
const MAX_CACHE_SIZE: usize = 512;

fn cache_path() -> PathBuf {
    crate::config::data_dir().join("query_embeddings.bin")
}

/// 缓存键：规范化后的查询
///
/// 原先直接用**原始查询字符串**当键，而线上几乎不会有两条完全相同的消息，
/// 于是缓存永远 miss——`vector_weight = 0.7` 实际作用在一个空列表上
/// （设计书 §5.2d）。规范化去掉 CQ 码、折叠空白、统一小写之后，
/// 同一话题的重复提问才会真正命中。
fn cache_key(query: &str) -> String {
    crate::util::normalize_cache_key(query)
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
        if offset + 4 > data.len() {
            break;
        }
        let key_len = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        if offset + key_len > data.len() {
            break;
        }
        let key = String::from_utf8_lossy(&data[offset..offset + key_len]).into_owned();
        offset += key_len;

        if offset + 4 > data.len() {
            break;
        }
        let dim = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        if offset + dim * 4 > data.len() {
            break;
        }
        let mut vec = Vec::with_capacity(dim);
        for d in 0..dim {
            let byte_offset = offset + d * 4;
            vec.push(f32::from_le_bytes([
                data[byte_offset],
                data[byte_offset + 1],
                data[byte_offset + 2],
                data[byte_offset + 3],
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

    if let Err(error) = crate::util::atomic_write(&path, buf) {
        tracing::warn!(error = %error, "写盘失败");
    }
}

/// 初始化查询向量缓存（从磁盘加载）
pub fn init_query_cache() {
    let cache = load_cache_from_disk();
    let mut guard = QUERY_CACHE.lock_recover();
    *guard = Some(QueryCache::from_entries(cache));
}

/// 获取缓存的查询向量（非阻塞，只读缓存）
pub fn get_cached_query_embedding(query: &str) -> Option<Vec<f32>> {
    let guard = QUERY_CACHE.lock_recover();
    guard.as_ref()?.get(&cache_key(query)).cloned()
}

/// 缓存查询向量（内存 + 异步写磁盘）
pub fn cache_query_embedding(query: String, embedding: Vec<f32>) {
    let should_save = {
        let mut guard = QUERY_CACHE.lock_recover();
        let cache = guard.get_or_insert_with(|| QueryCache::from_entries(load_cache_from_disk()));

        cache.insert(cache_key(&query), embedding, MAX_CACHE_SIZE);
        cache.entries.len().is_multiple_of(32) // 每 32 次写入保存一次
    };

    if should_save {
        let guard = QUERY_CACHE.lock_recover();
        if let Some(cache) = guard.as_ref() {
            save_cache_to_disk(&cache.entries);
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

    // 超时来自 `ai.request_timeout`：embedding 也在消息处理路径上，
    // 一个没有上限的请求会把整个串行队列拖住
    let resp_str = crate::util::post_json(
        &crate::ai::no_error_agent(),
        &url,
        &cfg.embedding.api_key,
        &json_body,
    )
    .ok()?;

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
    debug!(
        query_len = query.len(),
        "vector: query embedding generated and cached"
    );

    Some(vec)
}

/// 向量搜索：计算余弦相似度
pub fn search(query: &[f32], embeddings: &[(String, Vec<f32>)], top_k: usize) -> Vec<VectorResult> {
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

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(top_k);
    results
}

fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 缓存键折叠同一件事的不同写法——这是"向量路径恒为空"的修法
    #[test]
    fn equivalent_queries_share_one_cache_entry() {
        assert_eq!(cache_key("猫粮买哪种 "), cache_key("猫粮买哪种"));
        assert_eq!(cache_key("猫粮  买哪种"), cache_key("猫粮\n买哪种"));
        assert_eq!(
            cache_key("[CQ:at,qq=1] 猫粮买哪种"),
            cache_key("猫粮买哪种")
        );
        assert_eq!(cache_key("Rust 怎么装"), cache_key("rust 怎么装"));
    }

    /// 淘汰按插入顺序，而不是 HashMap 的任意顺序
    #[test]
    fn eviction_drops_the_oldest_not_an_arbitrary_key() {
        let mut cache = QueryCache {
            entries: HashMap::new(),
            order: VecDeque::new(),
        };
        let capacity = 3;
        for key in ["a", "b", "c", "d"] {
            cache.insert(key.to_string(), vec![1.0], capacity);
        }

        assert_eq!(cache.entries.len(), capacity, "容量必须被守住");
        assert!(cache.get("a").is_none(), "最旧的应被淘汰");
        for key in ["b", "c", "d"] {
            assert!(cache.get(key).is_some(), "{key} 不该被淘汰");
        }
    }

    /// 重复插入同一个键不该在顺序队列里留下两份
    #[test]
    fn reinserting_a_key_keeps_a_single_order_slot() {
        let mut cache = QueryCache {
            entries: HashMap::new(),
            order: VecDeque::new(),
        };
        cache.insert("a".to_string(), vec![1.0], 2);
        cache.insert("b".to_string(), vec![2.0], 2);
        cache.insert("a".to_string(), vec![3.0], 2);
        assert_eq!(cache.order.len(), 2, "同一个键不该在顺序队列里出现两次");
        cache.insert("c".to_string(), vec![4.0], 2);
        assert!(cache.get("a").is_none(), "a 最早插入，应先被淘汰");
        assert_eq!(cache.get("b"), Some(&vec![2.0]));
    }
}
