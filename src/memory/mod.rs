mod store;
mod operations;
mod extract;
mod review;
pub mod retrieval;
pub mod sqlite;
pub mod graph;

use std::collections::HashMap;

pub use store::*;
pub use operations::*;
pub use extract::*;
pub use review::*;

/// 初始化记忆系统（SQLite + JSON 迁移）
pub fn init(data_dir: &std::path::Path) {
    sqlite::init(data_dir);
    // 首次运行时从 JSON 迁移数据
    if sqlite::is_available() {
        let json_path = data_dir.join("memory.json");
        if json_path.exists() {
            sqlite::migrate_from_json(&json_path);
        }
    }
}

/// 语义检索记忆：双路检索（向量 + BM25）+ RRF 融合
///
/// 优先使用 SQLite 检索（含 embedding），fallback 到 JSON 全量加载
pub fn search_memories(user_id: u64, query: &str, top_k: usize) -> Vec<retrieval::RetrievalResult> {
    // 优先使用 SQLite 检索
    if sqlite::is_available() {
        // BM25 检索
        let entries = sqlite::search_memories(user_id, query, top_k);
        let documents: Vec<(String, String)> = entries.iter().enumerate()
            .map(|(i, e)| (format!("{}_{}", user_id, i), e.content.clone()))
            .collect();

        // 获取 embeddings
        let embeddings = sqlite::get_user_embeddings(user_id);

        // 使用 RRF 融合
        let config = retrieval::RetrievalConfig {
            top_k,
            vector_weight: 0.7,
            bm25_weight: 0.3,
            rrf_k: 60.0,
        };

        let mut results = retrieval::dual_path_retrieve(query, &documents, &embeddings, &config);

        // 填充 content
        let doc_map: HashMap<String, String> = documents.into_iter().collect();
        for result in &mut results {
            if let Some(content) = doc_map.get(&result.id) {
                result.content = content.clone();
            }
        }

        return results;
    }

    // Fallback: JSON 全量加载 + BM25 检索
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

    let embeddings: Vec<(String, Vec<f32>)> = Vec::new();

    let config = retrieval::RetrievalConfig {
        top_k,
        vector_weight: 0.7,
        bm25_weight: 0.3,
        rrf_k: 60.0,
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
