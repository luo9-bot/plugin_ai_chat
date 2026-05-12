mod store;
mod operations;
mod extract;
mod review;
pub mod retrieval;
pub mod graph;
pub mod embedding;
pub mod vector_store;

use std::collections::HashMap;
use tracing::debug;

pub use store::*;
pub use operations::*;
pub use extract::*;
pub use review::*;

/// 初始化记忆系统（JSON 元数据 + 二进制向量文件 + 知识图谱）
pub fn init(_data_dir: &std::path::Path) {
    store::MemoryStore::load();
    vector_store::init();
    graph::init();
    debug!("memory: JSON store + vectors.bin + graph initialized");
}

/// 语义检索记忆：双路检索 + 后置图门控 + 自适应阈值 + 智能回退
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

    // 从向量存储中按 content 匹配提取 embeddings
    let vector_map = vector_store::all_vectors();
    let embeddings: Vec<(String, Vec<f32>)> = user_mem
        .entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            let id = format!("{}_{}", user_id, i);
            vector_map.get(&entry.content).map(|emb| (id, emb.clone()))
        })
        .collect();

    // 如果没有存储的 embeddings，实时生成并存入向量文件
    let mut final_embeddings = embeddings;
    if final_embeddings.is_empty() && crate::config::get().embedding.enabled() {
        if let Some(_qe) = embedding::embed_text(query) {
            let doc_texts: Vec<String> = documents.iter().map(|(_, c)| c.clone()).collect();
            let doc_embeddings = embedding::embed_batch(&doc_texts);
            for (i, emb_opt) in doc_embeddings.into_iter().enumerate() {
                if let Some(emb) = emb_opt {
                    vector_store::add_vector(&documents[i].1, emb.clone());
                    final_embeddings.push((documents[i].0.clone(), emb));
                }
            }
        }
    }

    // 配置完整检索 pipeline
    let config = retrieval::RetrievalConfig {
        top_k,
        vector_weight: 0.7,
        bm25_weight: 0.3,
        rrf_k: 60.0,
        metadata_filter: None,
        threshold_config: Some(retrieval::ThresholdConfig::default()),
        posterior_graph_config: Some(retrieval::PosteriorGraphConfig::default()),
        enable_fallback: true,
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
