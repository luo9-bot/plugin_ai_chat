mod store;
mod operations;
mod extract;
mod review;
pub mod retrieval;

use std::collections::HashMap;

pub use store::*;
pub use operations::*;
pub use extract::*;
pub use review::*;

/// 语义检索记忆：双路检索（向量 + BM25）+ RRF 融合
pub fn search_memories(user_id: u64, query: &str, top_k: usize) -> Vec<retrieval::RetrievalResult> {
    let store = MemoryStore::load();
    let user_mem = match store.users.get(&user_id.to_string()) {
        Some(m) => m,
        None => return Vec::new(),
    };

    // 构建文档列表 (id, content)
    let documents: Vec<(String, String)> = user_mem
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| (format!("{}_{}", user_id, i), entry.content.clone()))
        .collect();

    // 当前没有 embedding 存储，只用 BM25
    // TODO: 后续添加 embedding 存储后启用向量检索
    let embeddings: Vec<(String, Vec<f32>)> = Vec::new();

    let config = retrieval::RetrievalConfig {
        top_k,
        vector_weight: 0.7,
        bm25_weight: 0.3,
        rrf_k: 60.0,
    };

    let mut results = retrieval::dual_path_retrieve(query, &documents, &embeddings, &config);

    // 填充 content 字段
    let doc_map: HashMap<String, String> = documents.into_iter().collect();
    for result in &mut results {
        if let Some(content) = doc_map.get(&result.id) {
            result.content = content.clone();
        }
    }

    results
}
