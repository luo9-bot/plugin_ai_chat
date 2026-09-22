pub(crate) mod cognitive_biases;
pub(crate) mod embedding;
mod extract;
pub(crate) mod graph;
mod operations;
pub(crate) mod ops_log;
pub(crate) mod retrieval;
mod review;
pub(crate) mod store;
pub(crate) mod unpredictability;
pub(crate) mod vector_store;

use std::collections::HashMap;

use crate::config;

pub(crate) use extract::*;
pub(crate) use operations::*;
pub(crate) use review::*;
pub(crate) use store::*;

/// 初始化记忆系统
pub(crate) fn init() {
    store::init();
    vector_store::init();
    retrieval::vector::init_query_cache();
    ops_log::init();
}

/// 语义检索记忆：双路检索 + 无状态遗忘曲线 + 后置图门控 + 自适应阈值 + 智能回退
///
/// 同时检索全局记忆和群特定记忆；被想起的记忆会得到强化（检索即强化）。
pub(crate) fn search_memories(
    user_id: u64,
    current_group_id: u64,
    query: &str,
    top_k: usize,
) -> Vec<retrieval::RetrievalResult> {
    search_memories_inner(user_id, current_group_id, query, top_k, false, 0.45)
}

pub(crate) fn search_memories_for_recall(
    user_id: u64,
    current_group_id: u64,
    query: &str,
    top_k: usize,
) -> Vec<retrieval::RetrievalResult> {
    search_memories_inner(user_id, current_group_id, query, top_k, true, 0.62)
}

fn search_memories_inner(
    user_id: u64,
    current_group_id: u64,
    query: &str,
    top_k: usize,
    stable_only: bool,
    min_vector_similarity: f64,
) -> Vec<retrieval::RetrievalResult> {
    let mut documents: Vec<(String, String)> = Vec::new();
    let mut meta: HashMap<String, retrieval::forgetting::MemoryMeta> = HashMap::new();

    // 全局记忆
    let global = store::load_user_memory(user_id);
    for (i, entry) in global.entries.iter().enumerate() {
        if stable_only && !recall_eligible(entry) { continue; }
        let id = format!("global_{}_{}", user_id, i);
        meta.insert(
            id.clone(),
            retrieval::forgetting::MemoryMeta {
                last_accessed: entry.last_accessed,
                access_count: entry.access_count,
                is_permanent: entry.importance == Importance::Permanent,
                is_important: entry.importance == Importance::Important,
            },
        );
        documents.push((id, entry.content.clone()));
    }

    // 群特定记忆
    if current_group_id > 0 {
        let group_user = store::load_group_user_memory(current_group_id, user_id);
        for (i, entry) in group_user.entries.iter().enumerate() {
            if stable_only && !recall_eligible(entry) { continue; }
            let id = format!("group_{}_{}_{}", current_group_id, user_id, i);
            meta.insert(
                id.clone(),
                retrieval::forgetting::MemoryMeta {
                    last_accessed: entry.last_accessed,
                    access_count: entry.access_count,
                    is_permanent: entry.importance == Importance::Permanent,
                    is_important: entry.importance == Importance::Important,
                },
            );
            documents.push((id, entry.content.clone()));
        }
    }

    if documents.is_empty() {
        return Vec::new();
    }

    // 逐个检查向量，避免全量克隆 HashMap
    let mut embeddings: Vec<(String, Vec<f32>)> = Vec::with_capacity(documents.len());
    let mut missing_indices: Vec<usize> = Vec::new();

    for (i, (id, content)) in documents.iter().enumerate() {
        if let Some(emb) = vector_store::get_vector(content) {
            embeddings.push((id.clone(), emb));
        } else {
            missing_indices.push(i);
        }
    }

    // 缺失的文档 embedding 在后台补充，不阻塞当前检索
    if !missing_indices.is_empty() && crate::config::get().embedding.enabled() {
        let missing_texts: Vec<String> = missing_indices
            .iter()
            .map(|&i| documents[i].1.clone())
            .collect();
        std::thread::spawn(move || {
            let doc_embeddings = embedding::embed_batch(&missing_texts);
            for (text, emb_opt) in missing_texts.into_iter().zip(doc_embeddings) {
                if let Some(emb) = emb_opt {
                    vector_store::add_vector(&text, emb);
                }
            }
        });
    }

    // 查询向量：如果缓存中没有，在后台生成（不阻塞当前检索）
    if retrieval::vector::get_cached_query_embedding(query).is_none()
        && crate::config::get().embedding.enabled()
    {
        let query_owned = query.to_string();
        std::thread::spawn(move || {
            retrieval::vector::generate_query_embedding(&query_owned);
        });
    }

    if embeddings.is_empty() {
        return dual_path_bm25_only(query, &documents, top_k);
    }

    let config = retrieval::RetrievalConfig {
        top_k,
        vector_weight: 0.7,
        bm25_weight: 0.3,
        rrf_k: 60.0,
        min_vector_similarity,
        threshold_config: Some(retrieval::ThresholdConfig::default()),
        posterior_graph_config: Some(retrieval::PosteriorGraphConfig::default()),
    };

    let mut results = retrieval::dual_path_retrieve(query, &documents, &embeddings, &config);

    let doc_map: HashMap<String, String> = documents.into_iter().collect();
    for result in &mut results {
        if let Some(content) = doc_map.get(&result.id) {
            result.content = content.clone();
        }
    }

    // 无状态遗忘曲线：陈旧的记忆被时间压低，常被想起的例外
    let mcfg = config::get().memory.clone();
    let fcfg = retrieval::forgetting::ForgettingConfig {
        enabled: mcfg.forgetting_enabled,
        half_life_secs: mcfg.forgetting_half_life_days * 86400.0,
        time_weight: mcfg.forgetting_time_weight,
        similarity_weight: mcfg.forgetting_similarity_weight,
        reinforcement_gain: mcfg.forgetting_reinforcement_gain,
    };
    let now = crate::util::now_secs();
    retrieval::forgetting::apply(&mut results, |id| meta.get(id).copied(), now, &fcfg);

    // 检索即强化：被想起的记忆延长半衰期（下一次更难忘记）
    if !stable_only && fcfg.enabled && !results.is_empty() {
        let hits: std::collections::HashSet<&str> =
            results.iter().map(|r| r.content.as_str()).collect();
        reinforce_hits(user_id, current_group_id, &hits);
    }

    // 应用认知偏差修正
    if !stable_only && config::get().humanity.cognitive_biases_enabled {
        let emotion = crate::emotion::get_state(user_id);
        let mut biases = cognitive_biases::load_biases();
        results = cognitive_biases::apply_cognitive_biases(results, &emotion.current, &mut biases);
        cognitive_biases::save_biases(&biases);
    }

    results
}

fn recall_eligible(entry: &MemoryEntry) -> bool {
    if entry.importance == Importance::Permanent
        || entry.emotional_impact.is_some_and(|impact| impact.abs() >= 6.0)
    {
        return true;
    }
    if entry.importance != Importance::Important { return false; }
    let transient = ["喝了", "吃了", "买了", "看了", "去了", "做了", "遇到", "刷到", "听了", "玩了", "睡了"];
    !transient.iter().any(|marker| entry.content.contains(marker))
}

/// 检索即强化：命中条目 access_count+1、刷新 last_accessed 并回写
fn reinforce_hits(user_id: u64, group_id: u64, hits: &std::collections::HashSet<&str>) {
    let reinforce_file = |entries: &mut Vec<MemoryEntry>| -> bool {
        let mut touched = false;
        for entry in entries.iter_mut() {
            if hits.contains(entry.content.as_str()) {
                operations::touch_entry(entry, entry.importance.clone());
                touched = true;
            }
        }
        touched
    };

    let mut global = store::load_user_memory(user_id);
    if reinforce_file(&mut global.entries) {
        store::save_user_memory(user_id, &global);
    }
    if group_id > 0 {
        let mut group_user = store::load_group_user_memory(group_id, user_id);
        if reinforce_file(&mut group_user.entries) {
            store::save_group_user_memory(group_id, user_id, &group_user);
        }
    }
}

fn dual_path_bm25_only(
    query: &str,
    documents: &[(String, String)],
    top_k: usize,
) -> Vec<retrieval::RetrievalResult> {
    let results = retrieval::bm25::search(query, documents, top_k * 2);
    results
        .into_iter()
        .take(top_k)
        .map(|r| retrieval::RetrievalResult {
            id: r.id.clone(),
            content: documents
                .iter()
                .find(|(id, _)| id == &r.id)
                .map(|(_, c)| c.clone())
                .unwrap_or_default(),
            score: r.score,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(content: &str, importance: Importance) -> MemoryEntry {
        MemoryEntry {
            content: content.to_string(),
            importance,
            created: 0,
            last_accessed: 0,
            access_count: 1,
            emotional_impact: None,
        }
    }

    #[test]
    fn automatic_recall_skips_one_time_drinks() {
        assert!(!recall_eligible(&entry("用户喝了可乐", Importance::Important)));
        assert!(recall_eligible(&entry("用户喜欢喝可乐", Importance::Important)));
        assert!(recall_eligible(&entry("用户要求永久记住这件事", Importance::Permanent)));
    }
}
