//! 记忆检索模块
//!
//! 双路检索：向量语义搜索 + BM25 关键词搜索
//! 使用 Weighted RRF 融合两路结果
//! 支持后置图门控、自适应阈值、智能回退

mod bm25;
mod vector;
mod fusion;
pub mod threshold;
pub mod posterior_graph;
pub mod fallback;

pub use fusion::RetrievalResult;
pub use threshold::ThresholdConfig;
pub use posterior_graph::PosteriorGraphConfig;

use std::collections::HashMap;

/// 检索配置（时序/人物/来源过滤）
#[derive(Debug, Clone, Default)]
pub struct MetadataFilter {
    /// 时间范围（unix 秒）
    pub time_from: Option<u64>,
    pub time_to: Option<u64>,
    /// 人物过滤（user_id）
    pub person: Option<u64>,
    /// 来源过滤
    pub source: Option<String>,
}

/// 双路检索配置
pub struct RetrievalConfig {
    pub top_k: usize,
    pub vector_weight: f64,
    pub bm25_weight: f64,
    pub rrf_k: f64,
    pub metadata_filter: Option<MetadataFilter>,
    pub threshold_config: Option<ThresholdConfig>,
    pub posterior_graph_config: Option<PosteriorGraphConfig>,
    pub enable_fallback: bool,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: 10,
            vector_weight: 0.7,
            bm25_weight: 0.3,
            rrf_k: 60.0,
            metadata_filter: None,
            threshold_config: None,
            posterior_graph_config: None,
            enable_fallback: true,
        }
    }
}

/// 双路检索（完整版）
///
/// 1. 元数据过滤
/// 2. BM25 关键词检索
/// 3. 向量语义检索
/// 4. Weighted RRF 融合
/// 5. 后置图门控
/// 6. 自适应阈值过滤
/// 7. 智能回退（结果不足时）
pub fn dual_path_retrieve(
    query: &str,
    memories: &[(String, String)], // (id, content)
    embeddings: &[(String, Vec<f32>)], // (id, embedding)
    config: &RetrievalConfig,
) -> Vec<RetrievalResult> {
    // 步骤1: 元数据过滤
    let filtered_memories: Vec<(String, String)> = if let Some(ref filter) = config.metadata_filter {
        apply_metadata_filter(memories, filter)
    } else {
        memories.to_vec()
    };

    if filtered_memories.is_empty() {
        return Vec::new();
    }

    // 步骤2: BM25 关键词检索
    let bm25_results = bm25::search(query, &filtered_memories, config.top_k * 2);

    // 步骤3: 向量语义检索
    let vector_results = if let Some(query_embedding) = vector::embed_query(query) {
        vector::search(&query_embedding, embeddings, config.top_k * 2)
    } else {
        Vec::new()
    };

    // 步骤4: Weighted RRF 融合
    let mut fused = fusion::weighted_rrf_fusion(
        &vector_results,
        &bm25_results,
        config.rrf_k,
        config.vector_weight,
        config.bm25_weight,
    );

    // 填充 content
    let doc_map: HashMap<String, String> = filtered_memories.into_iter().collect();
    for result in &mut fused {
        if let Some(content) = doc_map.get(&result.id) {
            result.content = content.clone();
        }
    }

    // 步骤5: 后置图门控
    if let Some(ref pg_config) = config.posterior_graph_config {
        posterior_graph::apply_posterior_graph_gate(&mut fused, pg_config);
    }

    // 步骤6: 自适应阈值过滤
    if let Some(ref t_config) = config.threshold_config {
        threshold::adaptive_threshold_filter(&mut fused, t_config);
    }

    // 步骤7: 智能回退
    if config.enable_fallback {
        let fallback_cfg = fallback::FallbackConfig::default();
        let fallback_queries = fallback::smart_fallback(query, fused.len(), &fallback_cfg);
        for fb_query in fallback_queries {
            if fused.len() >= config.top_k {
                break;
            }
            // 用简化后的查询再跑一次 BM25
            let fb_results = bm25::search(&fb_query, &[], config.top_k);
            let fb_map: HashMap<String, String> = HashMap::new();
            for r in fb_results {
                if let Some(content) = fb_map.get(&r.id) {
                    if !fused.iter().any(|f| f.id == r.id) {
                        fused.push(RetrievalResult {
                            id: r.id,
                            content: content.clone(),
                            score: r.score * 0.5, // 回退结果降权
                            source: "fallback",
                        });
                    }
                }
            }
        }
    }

    // 取 top_k
    fused.truncate(config.top_k);
    fused
}

/// 应用元数据过滤
fn apply_metadata_filter(
    memories: &[(String, String)],
    filter: &MetadataFilter,
) -> Vec<(String, String)> {
    let mut filtered = Vec::new();
    for (id, content) in memories {
        let mut keep = true;
        if let Some(ref person) = filter.person {
            // 如果 id 包含 user_id 信息则匹配
            if !id.contains(&person.to_string()) {
                keep = false;
            }
        }
        if keep {
            filtered.push((id.clone(), content.clone()));
        }
    }
    filtered
}
