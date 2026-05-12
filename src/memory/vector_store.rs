//! Embedding 向量存储
//!
//! 向量数据独立存储在 `vectors.bin` 二进制文件中，
//! JSON 元数据（memory.json）只存储文本和属性，不存向量。
//!
//! 文件格式（所有多字节值为小端序）：
//!   [magic: u32 = 0x56435452]  ("VCTR")
//!   [version: u32 = 1]
//!   [count: u32]
//!   entries: [
//!     [content_len: u32][content: UTF-8 bytes][dim: u32][data: f32[dim]]
//!     ...
//!   ]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{debug, warn};

static CACHE: Mutex<Option<HashMap<String, Vec<f32>>>> = Mutex::new(None);

const MAGIC: u32 = 0x56435452;
const VERSION: u32 = 1;

fn vectors_path() -> PathBuf {
    crate::config::data_dir().join("vectors.bin")
}

/// 初始化：预加载向量文件到缓存
pub fn init() {
    let mut guard = CACHE.lock().unwrap();
    if guard.is_some() {
        return;
    }
    *guard = Some(load_file());
    debug!("vector_store: loaded {} vectors", guard.as_ref().map_or(0, |m| m.len()));
}

fn load_file() -> HashMap<String, Vec<f32>> {
    let path = vectors_path();
    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };

    if data.len() < 12 {
        return HashMap::new();
    }

    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if magic != MAGIC || version != VERSION {
        warn!("vector_store: invalid file header");
        return HashMap::new();
    }

    let count = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let mut map = HashMap::with_capacity(count);
    let mut offset = 12;

    for _ in 0..count {
        if offset + 4 > data.len() {
            break;
        }
        let content_len = u32::from_le_bytes([
            data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
        ]) as usize;
        offset += 4;

        if offset + content_len + 4 > data.len() {
            break;
        }
        let content = match std::str::from_utf8(&data[offset..offset + content_len]) {
            Ok(s) => s.to_string(),
            Err(_) => {
                offset += content_len + 4;
                continue;
            }
        };
        offset += content_len;

        let dim = u32::from_le_bytes([
            data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
        ]) as usize;
        offset += 4;

        if offset + dim * 4 > data.len() {
            break;
        }

        let mut vec = Vec::with_capacity(dim);
        for i in 0..dim {
            let byte_offset = offset + i * 4;
            vec.push(f32::from_le_bytes([
                data[byte_offset],
                data[byte_offset + 1],
                data[byte_offset + 2],
                data[byte_offset + 3],
            ]));
        }
        offset += dim * 4;

        map.insert(content, vec);
    }

    map
}

fn save_file(vectors: &HashMap<String, Vec<f32>>) {
    let path = vectors_path();
    let mut buf = Vec::new();

    // header
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.extend_from_slice(&VERSION.to_le_bytes());
    buf.extend_from_slice(&(vectors.len() as u32).to_le_bytes());

    // entries
    for (content, vec) in vectors {
        let content_bytes = content.as_bytes();
        buf.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(content_bytes);
        buf.extend_from_slice(&(vec.len() as u32).to_le_bytes());
        for &v in vec {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }

    std::fs::write(&path, buf).ok();
}

/// 添加一个向量（content 作为唯一标识，覆盖旧值）
pub fn add_vector(content: &str, embedding: Vec<f32>) {
    let mut guard = CACHE.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    map.insert(content.to_string(), embedding);
    save_file(map);
}

/// 按内容获取向量
pub fn get_vector(content: &str) -> Option<Vec<f32>> {
    let guard = CACHE.lock().unwrap();
    let map = guard.as_ref()?;
    map.get(content).cloned()
}

/// 获取所有向量的克隆（供检索使用）
pub fn all_vectors() -> HashMap<String, Vec<f32>> {
    let guard = CACHE.lock().unwrap();
    guard.clone().unwrap_or_default()
}

/// 移除指定内容的向量
pub fn remove_vector(content: &str) {
    let mut guard = CACHE.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    map.remove(content);
    save_file(map);
}

/// 向量总数
pub fn count() -> usize {
    let guard = CACHE.lock().unwrap();
    guard.as_ref().map_or(0, |m| m.len())
}
