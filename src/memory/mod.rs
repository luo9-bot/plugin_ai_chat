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

    if documents.is_empty() {
        return Vec::new();
    }

    // 从向量存储中按 content 匹配提取已有 embeddings
    let vector_map = vector_store::all_vectors();
    let mut embeddings: Vec<(String, Vec<f32>)> = Vec::with_capacity(documents.len());
    let mut missing_indices: Vec<usize> = Vec::new();

    for (i, (id, content)) in documents.iter().enumerate() {
        if let Some(emb) = vector_map.get(content) {
            embeddings.push((id.clone(), emb.clone()));
        } else {
            missing_indices.push(i);
        }
    }

    // 只对缺失的文档生成 embedding，而非全量重做
    if !missing_indices.is_empty() && crate::config::get().embedding.enabled() {
        let missing_texts: Vec<String> = missing_indices.iter()
            .map(|&i| documents[i].1.clone())
            .collect();
        let doc_embeddings = embedding::embed_batch(&missing_texts);
        for (offset, emb_opt) in doc_embeddings.into_iter().enumerate() {
            if let Some(emb) = emb_opt {
                let idx = missing_indices[offset];
                vector_store::add_vector(&documents[idx].1, emb.clone());
                embeddings.push((documents[idx].0.clone(), emb));
            }
        }
    }

    // 没有可用向量时，降级为纯 BM25
    if embeddings.is_empty() {
        return dual_path_bm25_only(query, &documents, top_k);
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

    let mut results = retrieval::dual_path_retrieve(query, &documents, &embeddings, &config);

    let doc_map: HashMap<String, String> = documents.into_iter().collect();
    for result in &mut results {
        if let Some(content) = doc_map.get(&result.id) {
            result.content = content.clone();
        }
    }

    results
}

/// 纯 BM25 降级检索（当无可用向量时）
fn dual_path_bm25_only(query: &str, documents: &[(String, String)], top_k: usize) -> Vec<retrieval::RetrievalResult> {
    let results = retrieval::bm25::search(query, documents, top_k * 2);
    results.into_iter().take(top_k).map(|r| {
        retrieval::RetrievalResult {
            id: r.id.clone(),
            content: documents.iter()
                .find(|(id, _)| id == &r.id)
                .map(|(_, c)| c.clone())
                .unwrap_or_default(),
            score: r.score,
            source: "bm25",
        }
    }).collect()
}
