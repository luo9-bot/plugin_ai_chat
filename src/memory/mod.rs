mod store;
mod operations;
mod extract;
mod review;
pub mod retrieval;
pub mod graph;
pub mod embedding;

use std::collections::HashMap;
use tracing::{debug, info};

pub use store::*;
pub use operations::*;
pub use extract::*;
pub use review::*;

/// 初始化记忆系统（纯 JSON 存储）
pub fn init(_data_dir: &std::path::Path) {
    // 预加载缓存，确保 memory.json 可用
    store::MemoryStore::load();
    debug!("memory: JSON store initialized");
}

/// 语义检索记忆：双路检索（向量 + BM25）+ RRF 融合
///
/// 从 JSON 文件中加载记忆，支持可选的 embedding 向量语义检索
pub fn search_memories(user_id: u64, query: &str, top_k: usize) -> Vec<retrieval::RetrievalResult> {
    let store = MemoryStore::load();
    let user_mem = match store.users.get(&user_id.to_string()) {
        Some(m) => m,
        None => return Vec::new(),
    };

    let documents: Vec<(String, String)> = user_mem
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| (format!("{}_{}", user_id, i), entry.content.clone()))
        .collect();

    // 提取已有的 embeddings
    let embeddings: Vec<(String, Vec<f32>)> = user_mem
        .entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            entry.embedding.clone().map(|emb| (format!("{}_{}", user_id, i), emb))
        })
        .collect();

    // 如果没有存储的 embeddings，尝试实时生成
    let mut final_embeddings = embeddings;
    if final_embeddings.is_empty() && crate::config::get().embedding.enabled() {
        if let Some(_query_embedding) = embedding::embed_text(query) {
            let doc_texts: Vec<String> = documents.iter().map(|(_, c)| c.clone()).collect();
            let doc_embeddings = embedding::embed_batch(&doc_texts);
            for (i, emb_opt) in doc_embeddings.into_iter().enumerate() {
                if let Some(emb) = emb_opt {
                    final_embeddings.push((documents[i].0.clone(), emb));
                }
            }
        }
    }

    let config = retrieval::RetrievalConfig {
        top_k,
        vector_weight: 0.7,
        bm25_weight: 0.3,
        rrf_k: 60.0,
    };

    let mut results = retrieval::dual_path_retrieve(query, &documents, &final_embeddings, &config);

    let doc_map: HashMap<String, String> = documents.into_iter().collect();
    for result in &mut results {
        if let Some(content) = doc_map.get(&result.id) {
            result.content = content.clone();
        }
    }

    results
}

/// 存储记忆时自动生成 embedding（原子写入 JSON）
pub fn store_memory_with_embedding(user_id: u64, content: &str, importance: store::Importance) {
    let now = crate::util::now_secs();
    
    // 生成 embedding（可选）
    let embedding = if crate::config::get().embedding.enabled() {
        embedding::embed_text(content)
    } else {
        None
    };

    let entry = store::MemoryEntry {
        content: content.to_string(),
        importance,
        created: now,
        last_accessed: now,
        access_count: 0,
        embedding,
    };

    let content_preview: String = content.chars().take(40).collect();

    // 原子写入 JSON（持锁）
    let mut store = store::MemoryStore::load();
    store.get_user_mut(user_id).entries.push(entry);
    store.save();
    info!(user_id, content = %content_preview, "store_memory: saved to JSON");
}
