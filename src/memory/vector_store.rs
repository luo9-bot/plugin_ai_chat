//! Embedding 向量存储
//!
//! SQ8 量化 + Flat 回退索引，类似 Faiss IndexIDMap2(ScalarQuantizer(QT_8bit))。
//!
//! 特性：
//! - SQ8 标量量化（float32 → int8，min-max 映射）
//! - SHA1 稳定的 int64 ID
//! - Flat 回退索引（SQ8 训练完成前使用）
//! - 渐进训练（储水池采样）
//! - Append-only 磁盘存储
//! - L2 归一化（IP = Cosine）
//! - 线程安全

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{debug, warn};

// ── 文件格式 ────────────────────────────────────────────────────
// [magic: u32 = 0x56435452] ("VCTR")
// [version: u32 = 1]
// [dimension: u32]
// [trained: u8] (0=未训练用Flat, 1=已训练用SQ8)
// [quant_params: 仅 trained=1 时存在]
//   [mins: f32[dimension]]
//   [maxs: f32[dimension]]
// [count: u32]
// entries: [
//   [id: u64]
//   [vector_data: 取决于 trained]:
//     trained=0: [f32[dimension]] (raw)
//     trained=1: [u8[dimension]] (quantized)
//   ...
// ]

const MAGIC: u32 = 0x56435452;
const VERSION: u32 = 1;
/// SQ8 训练所需的最小样本数
const MIN_TRAIN_SAMPLES: usize = 40;
/// 储水池采样上限
const RESERVOIR_CAPACITY: usize = 10000;
/// 缓冲区刷新阈值
const BUFFER_SIZE: usize = 1024;

/// SQ8 量化参数
#[derive(Debug, Clone)]
struct QuantParams {
    mins: Vec<f32>,
    maxs: Vec<f32>,
}

impl QuantParams {
    fn from_samples(samples: &[Vec<f32>]) -> Self {
        let dim = samples[0].len();
        let mut mins = vec![f32::MAX; dim];
        let mut maxs = vec![f32::MIN; dim];
        for v in samples {
            for d in 0..dim {
                mins[d] = mins[d].min(v[d]);
                maxs[d] = maxs[d].max(v[d]);
            }
        }
        // 防止除零
        for d in 0..dim {
            if (maxs[d] - mins[d]).abs() < 1e-10 {
                maxs[d] = mins[d] + 1.0;
            }
        }
        Self { mins, maxs }
    }

    fn quantize(&self, vector: &[f32]) -> Vec<u8> {
        vector
            .iter()
            .enumerate()
            .map(|(d, &v)| {
                let normalized = (v - self.mins[d]) / (self.maxs[d] - self.mins[d]);
                (normalized * 255.0).round().clamp(0.0, 255.0) as u8
            })
            .collect()
    }

    fn dequantize(&self, quantized: &[u8]) -> Vec<f32> {
        quantized
            .iter()
            .enumerate()
            .map(|(d, &q)| {
                let normalized = q as f32 / 255.0;
                self.mins[d] + normalized * (self.maxs[d] - self.mins[d])
            })
            .collect()
    }
}

/// 向量存储
pub struct VectorStore {
    dim: usize,
    trained: bool,
    quant_params: Option<QuantParams>,
    /// 主索引：int64 ID -> (content_hash, 原始向量或量化向量)
    id_to_content: HashMap<i64, String>,
    /// content -> 原始向量（用于Flat回退和训练）
    raw_vectors: HashMap<String, Vec<f32>>,
    /// 量化后的向量（SQ8训练完成后使用）
    quantized_vectors: HashMap<String, Vec<u8>>,
    /// 储水池（渐进训练用）
    reservoir: Vec<Vec<f32>>,
    reservoir_seen: usize,
    /// 写缓冲区
    write_buffer: Vec<(i64, String, Vec<f32>)>,
    /// 统计
    total_added: usize,
}

impl VectorStore {
    fn new(dim: usize) -> Self {
        Self {
            dim,
            trained: false,
            quant_params: None,
            id_to_content: HashMap::new(),
            raw_vectors: HashMap::new(),
            quantized_vectors: HashMap::new(),
            reservoir: Vec::with_capacity(RESERVOIR_CAPACITY),
            reservoir_seen: 0,
            write_buffer: Vec::with_capacity(BUFFER_SIZE),
            total_added: 0,
        }
    }

    /// 从 content 生成稳定的 int64 ID (SHA1 截断)
    fn generate_id(content: &str) -> i64 {
        use sha1::Digest;
        let mut hasher = sha1::Sha1::new();
        hasher.update(content.as_bytes());
        let bytes = hasher.finalize();
        let val = i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        val & 0x7FFFFFFFFFFFFFFF
    }

    /// L2 归一化向量（原地）
    fn l2_normalize(v: &mut [f32]) {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-10 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
    }

    /// 添加向量（以 content 为唯一键）
    fn add(&mut self, content: &str, vector: Vec<f32>) -> bool {
        if vector.len() != self.dim {
            warn!("vector_store: dimension mismatch, expected {}, got {}", self.dim, vector.len());
            return false;
        }

        let id = Self::generate_id(content);
        let mut vec = vector;
        Self::l2_normalize(&mut vec);

        let exists = self.raw_vectors.contains_key(content);
        self.raw_vectors.insert(content.to_string(), vec.clone());
        self.id_to_content.insert(id, content.to_string());

        if !exists {
            self.total_added += 1;
            // 写缓冲区
            self.write_buffer
                .push((id, content.to_string(), vec.clone()));
            // 储水池采样
            self.reservoir_seen += 1;
            if self.reservoir.len() < RESERVOIR_CAPACITY {
                self.reservoir.push(vec);
            } else {
                // 随机替换
                let j = fastrand::usize(0..=self.reservoir_seen);
                if j < RESERVOIR_CAPACITY {
                    self.reservoir[j] = vec;
                }
            }
            // 尝试训练
            if !self.trained && self.reservoir.len() >= MIN_TRAIN_SAMPLES {
                self.train();
            }
        }
        true
    }

    /// 渐进训练 SQ8 量化参数
    fn train(&mut self) {
        if self.reservoir.len() < MIN_TRAIN_SAMPLES {
            return;
        }
        let params = QuantParams::from_samples(&self.reservoir);
        // 量化所有已有向量
        let mut quantized = HashMap::new();
        for (content, raw) in &self.raw_vectors {
            let q = params.quantize(raw);
            quantized.insert(content.clone(), q);
        }
        self.quant_params = Some(params);
        self.quantized_vectors = quantized;
        self.trained = true;
        debug!(
            vectors = self.raw_vectors.len(),
            dim = self.dim,
            "vector_store: SQ8 training completed"
        );
    }

    /// 搜索 top_k 最相似向量（余弦相似度）
    fn search(&self, query: &[f32], top_k: usize) -> Vec<SearchResult> {
        if self.raw_vectors.is_empty() || query.is_empty() {
            return Vec::new();
        }

        let query_norm: f32 = query.iter().map(|x| x * x).sum::<f32>().sqrt();
        if query_norm < 1e-10 {
            return Vec::new();
        }

        let mut results: Vec<SearchResult> = self
            .raw_vectors
            .iter()
            .map(|(content, vec)| {
                let dot: f32 = query.iter().zip(vec.iter()).map(|(a, b)| a * b).sum();
                let score = (dot / query_norm) as f64;
                SearchResult {
                    content: content.clone(),
                    score,
                }
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }

    /// 使用 SQ8 近似搜索
    fn search_quantized(&self, query: &[f32], top_k: usize) -> Vec<SearchResult> {
        let params = match &self.quant_params {
            Some(p) => p,
            None => return self.search(query, top_k),
        };

        let quantized_query = params.quantize(query);
        let query_deq = params.dequantize(&quantized_query);

        let mut results: Vec<SearchResult> = self
            .quantized_vectors
            .iter()
            .map(|(content, qvec)| {
                let deq = params.dequantize(qvec);
                let dot: f32 = query_deq.iter().zip(deq.iter()).map(|(a, b)| a * b).sum();
                let score = dot as f64;
                SearchResult {
                    content: content.clone(),
                    score,
                }
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }

    /// 查询是否包含某 content
    fn contains(&self, content: &str) -> bool {
        self.raw_vectors.contains_key(content)
    }

    /// 移除向量
    fn remove(&mut self, content: &str) {
        if let Some(id) = self.id_to_content.iter().find_map(|(k, v)| {
            if v == content {
                Some(*k)
            } else {
                None
            }
        }) {
            self.id_to_content.remove(&id);
        }
        self.raw_vectors.remove(content);
        self.quantized_vectors.remove(content);
    }

    /// 返回数据量
    fn len(&self) -> usize {
        self.raw_vectors.len()
    }
}

/// 搜索结果
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub content: String,
    pub score: f64,
}

// ── 持久化 ──────────────────────────────────────────────────────

fn vectors_path() -> PathBuf {
    crate::config::data_dir().join("vectors.bin")
}

fn load_from_disk() -> (HashMap<String, Vec<f32>>, usize) {
    let path = vectors_path();
    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(_) => return (HashMap::new(), 0),
    };

    if data.len() < 14 {
        return (HashMap::new(), 0);
    }

    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if magic != MAGIC || version != VERSION {
        warn!("vector_store: invalid file header");
        return (HashMap::new(), 0);
    }

    let dim = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let trained = data[12] != 0;

    let mut offset = 13;

    // 读取量化参数（如果已训练）
    let quant_params = if trained {
        if offset + dim * 8 > data.len() {
            return (HashMap::new(), 0);
        }
        let mut mins = Vec::with_capacity(dim);
        let mut maxs = Vec::with_capacity(dim);
        for d in 0..dim {
            let byte_offset = offset + d * 4;
            mins.push(f32::from_le_bytes([
                data[byte_offset],
                data[byte_offset + 1],
                data[byte_offset + 2],
                data[byte_offset + 3],
            ]));
        }
        offset += dim * 4;
        for d in 0..dim {
            let byte_offset = offset + d * 4;
            maxs.push(f32::from_le_bytes([
                data[byte_offset],
                data[byte_offset + 1],
                data[byte_offset + 2],
                data[byte_offset + 3],
            ]));
        }
        offset += dim * 4;
        Some(QuantParams { mins, maxs })
    } else {
        None
    };

    if offset + 4 > data.len() {
        return (HashMap::new(), 0);
    }
    let count = u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]) as usize;
    offset += 4;

    let mut vectors = HashMap::with_capacity(count);

    for _ in 0..count {
        if offset + 8 > data.len() {
            break;
        }
        let _id = i64::from_le_bytes([
            data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
            data[offset + 4], data[offset + 5], data[offset + 6], data[offset + 7],
        ]);
        offset += 8;

        if trained {
            if let Some(ref params) = quant_params {
                if offset + dim > data.len() {
                    break;
                }
                let qvec: Vec<u8> = data[offset..offset + dim].to_vec();
                let deq = params.dequantize(&qvec);
                // content 无法从文件恢复，这里用 ID 的字符串形式
                vectors.insert(_id.to_string(), deq);
                offset += dim;
            }
        } else {
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
            vectors.insert(_id.to_string(), vec);
            offset += dim * 4;
        }
    }

    debug!(count = vectors.len(), dim, trained, "vector_store: loaded from disk");
    (vectors, dim)
}

fn save_to_disk(store: &VectorStore) {
    let path = vectors_path();
    let mut buf = Vec::new();

    // Header
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.extend_from_slice(&VERSION.to_le_bytes());
    buf.extend_from_slice(&(store.dim as u32).to_le_bytes());
    buf.push(if store.trained { 1 } else { 0 });

    // 量化参数
    if let Some(ref params) = store.quant_params {
        for &v in &params.mins {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        for &v in &params.maxs {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }

    // 数据
    let count = store.raw_vectors.len();
    buf.extend_from_slice(&(count as u32).to_le_bytes());

    for (content, raw) in &store.raw_vectors {
        let id = VectorStore::generate_id(content);
        buf.extend_from_slice(&id.to_le_bytes());

        if store.trained {
            if let Some(ref params) = store.quant_params {
                let q = params.quantize(raw);
                buf.extend_from_slice(&q);
            }
        } else {
            for &v in raw {
                buf.extend_from_slice(&v.to_le_bytes());
            }
        }
    }

    std::fs::write(&path, buf).ok();
}

// ── 全局单例 ────────────────────────────────────────────────────

static STORE: Mutex<Option<VectorStore>> = Mutex::new(None);

/// 初始化向量存储
pub fn init() {
    let mut guard = STORE.lock().unwrap();
    if guard.is_some() {
        return;
    }

    let (vectors, dim) = load_from_disk();
    if vectors.is_empty() {
        // 从配置中获取维度
        let cfg_dim = 2048; // doubao-embedding-vision 默认维度
        *guard = Some(VectorStore::new(cfg_dim));
        debug!("vector_store: initialized new store, dim={}", cfg_dim);
        return;
    }

    let mut store = VectorStore::new(dim);
    let mut has_content = false;
    for (id_str, vec) in &vectors {
        // 尝试恢复 content，如果没有就用 ID 字符串
        store.raw_vectors.insert(id_str.clone(), vec.clone());
        if let Ok(id) = id_str.parse::<i64>() {
            store.id_to_content.insert(id, id_str.clone());
        }
        store.total_added += 1;
        has_content = true;
    }

    if has_content && store.reservoir.len() >= MIN_TRAIN_SAMPLES {
        store.train();
    }

    *guard = Some(store);
    debug!("vector_store: initialized from disk, {} vectors, dim={}", vectors.len(), dim);
}

/// 添加向量（content 作为唯一键）
pub fn add_vector(content: &str, embedding: Vec<f32>) {
    let mut guard = STORE.lock().unwrap();
    let store = guard.as_mut().expect("vector_store not initialized");
    store.add(content, embedding);
    // 每 BUFFER_SIZE 次写入刷新磁盘
    if store.write_buffer.len() >= BUFFER_SIZE || store.total_added % 10 == 0 {
        save_to_disk(store);
        store.write_buffer.clear();
    }
}

/// 按内容获取向量
pub fn get_vector(content: &str) -> Option<Vec<f32>> {
    let guard = STORE.lock().unwrap();
    let store = guard.as_ref()?;
    store.raw_vectors.get(content).cloned()
}

/// 搜索最相似向量
pub fn search(query: &[f32], top_k: usize) -> Vec<SearchResult> {
    let guard = STORE.lock().unwrap();
    let store = guard.as_ref().expect("vector_store not initialized");
    if store.trained {
        store.search_quantized(query, top_k)
    } else {
        store.search(query, top_k)
    }
}

/// 按 content 匹配获取所有向量
pub fn all_vectors() -> HashMap<String, Vec<f32>> {
    let guard = STORE.lock().unwrap();
    let store = guard.as_ref().expect("vector_store not initialized");
    store.raw_vectors.clone()
}

/// 移除向量
pub fn remove_vector(content: &str) {
    let mut guard = STORE.lock().unwrap();
    let store = guard.as_mut().expect("vector_store not initialized");
    store.remove(content);
    save_to_disk(store);
}

/// 向量总数
pub fn count() -> usize {
    let guard = STORE.lock().unwrap();
    guard.as_ref().map_or(0, |s| s.len())
}

/// 是否已训练 SQ8
pub fn is_trained() -> bool {
    let guard = STORE.lock().unwrap();
    guard.as_ref().map_or(false, |s| s.trained)
}

/// 强制刷新到磁盘
pub fn flush() {
    let guard = STORE.lock().unwrap();
    if let Some(store) = guard.as_ref() {
        save_to_disk(store);
    }
}
