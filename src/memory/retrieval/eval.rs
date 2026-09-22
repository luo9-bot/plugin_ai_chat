//! 检索质量评测
//!
//! 设计书 §5 的核心指控是"**不可测量的检索等于随机**"：RRF 的 `k=60`、
//! 两路权重、阈值倍数等常量都没有测量支撑，而项目自己提到过"15.1% 答错人率"
//! 却没有任何对应机制。
//!
//! 这里是那个缺失的机制：给定一批 `(查询, 相关记忆 id)`，算出
//! recall@k / MRR / nDCG@k，从而让检索改动**有回归底线**，也让常量第一次
//! 可以被参数扫描。
//!
//! 目前整块在 `#[cfg(test)]` 下：真正有意义的语料是线上累积的
//! `episode_recall`（设计书 §5.3.5 的 `recall_eval.jsonl`），在那之前
//! 它只作为测试夹具存在，避免留下无人读的"生产代码"。

use std::collections::HashSet;

/// 一个评测用例
pub(crate) struct EvalCase {
    pub query: String,
    /// 该查询下真正相关的记忆 id
    pub relevant: Vec<String>,
}

/// 检索质量指标
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RecallMetrics {
    /// 前 k 条里命中了多少比例的相关项（对每个查询取平均）
    pub recall_at_k: f64,
    /// 第一个相关项排名的倒数（对每个查询取平均）
    pub mrr: f64,
    pub ndcg_at_k: f64,
}

impl RecallMetrics {
    pub(crate) fn describe(&self, k: usize) -> String {
        format!(
            "recall@{k}={:.3} mrr={:.3} ndcg@{k}={:.3}",
            self.recall_at_k, self.mrr, self.ndcg_at_k
        )
    }
}

/// 对一批用例求指标
///
/// `retrieve` 返回**按相关性降序**的 id 列表（就是检索管线的输出顺序）。
pub(crate) fn evaluate(
    cases: &[EvalCase],
    k: usize,
    retrieve: impl Fn(&str) -> Vec<String>,
) -> RecallMetrics {
    if cases.is_empty() || k == 0 {
        return RecallMetrics {
            recall_at_k: 0.0,
            mrr: 0.0,
            ndcg_at_k: 0.0,
        };
    }

    let mut recall_sum = 0.0;
    let mut mrr_sum = 0.0;
    let mut ndcg_sum = 0.0;

    for case in cases {
        let ranked = retrieve(&case.query);
        let relevant: HashSet<&String> = case.relevant.iter().collect();
        if relevant.is_empty() {
            continue;
        }

        let top: Vec<&String> = ranked.iter().take(k).collect();
        let hits = top.iter().filter(|id| relevant.contains(*id)).count();
        recall_sum += hits as f64 / relevant.len() as f64;

        // MRR：第一个命中项的排名倒数
        mrr_sum += ranked
            .iter()
            .position(|id| relevant.contains(id))
            .map(|rank| 1.0 / (rank + 1) as f64)
            .unwrap_or(0.0);

        // nDCG@k：二元相关性下的 DCG / 理想 DCG
        let dcg: f64 = top
            .iter()
            .enumerate()
            .filter(|(_, id)| relevant.contains(*id))
            .map(|(rank, _)| 1.0 / ((rank + 2) as f64).log2())
            .sum();
        let ideal_hits = relevant.len().min(k);
        let idcg: f64 = (0..ideal_hits)
            .map(|rank| 1.0 / ((rank + 2) as f64).log2())
            .sum();
        ndcg_sum += if idcg > 0.0 { dcg / idcg } else { 0.0 };
    }

    let n = cases.len() as f64;
    RecallMetrics {
        recall_at_k: recall_sum / n,
        mrr: mrr_sum / n,
        ndcg_at_k: ndcg_sum / n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::retrieval::{RetrievalConfig, dual_path_retrieve};

    /// 一份带标注的小语料：相关项与查询共享用词，因此"相关"是构造出来的，
    /// 不依赖任何主观判断。
    fn corpus() -> Vec<EvalCase> {
        vec![
            EvalCase {
                query: "猫粮买哪种".into(),
                relevant: vec!["m_cat_food".into(), "m_cat".into()],
            },
            EvalCase {
                query: "狗粮买哪种".into(),
                relevant: vec!["m_dog_food".into(), "m_dog".into()],
            },
            EvalCase {
                query: "rust 怎么装".into(),
                relevant: vec!["m_rust_install".into()],
            },
            EvalCase {
                query: "她喜欢喝什么".into(),
                relevant: vec!["m_drink".into()],
            },
        ]
    }

    fn documents() -> Vec<(String, String)> {
        vec![
            ("m_cat_food".into(), "猫粮要买无谷的，猫吃得好".into()),
            ("m_cat".into(), "她养了一只猫，猫叫豆奶".into()),
            ("m_dog_food".into(), "狗粮要买大颗粒的，狗嚼得动".into()),
            ("m_dog".into(), "邻居家的狗很吵".into()),
            ("m_rust_install".into(), "rust 怎么装：用 rustup".into()),
            ("m_drink".into(), "她喜欢喝冰美式，不加糖".into()),
            ("m_unrelated".into(), "今天天气不错".into()),
        ]
    }

    /// 跑真实检索管线（指定两路权重），返回按分数降序的 id
    fn retrieve_with(query: &str, vector_weight: f64, bm25_weight: f64) -> Vec<String> {
        let config = RetrievalConfig {
            top_k: 5,
            vector_weight,
            bm25_weight,
            threshold_config: Some(crate::memory::retrieval::ThresholdConfig::default()),
            posterior_graph_config: None,
            ..Default::default()
        };
        dual_path_retrieve(query, &documents(), &[], &config)
            .into_iter()
            .map(|result| result.id)
            .collect()
    }

    /// 无向量时用的默认权重
    fn retrieve_with_pipeline(query: &str) -> Vec<String> {
        retrieve_with(query, 0.7, 0.3)
    }

    /// 评测机制本身必须正确：完美排序拿满分，无关排序拿零分
    #[test]
    fn metrics_reward_perfect_ranking_and_punish_unrelated() {
        let cases = vec![EvalCase {
            query: "q".into(),
            relevant: vec!["a".into(), "b".into()],
        }];

        let perfect = evaluate(&cases, 5, |_| vec!["a".into(), "b".into()]);
        assert_eq!(perfect.recall_at_k, 1.0);
        assert_eq!(perfect.mrr, 1.0);
        assert!((perfect.ndcg_at_k - 1.0).abs() < 1e-9);

        let unrelated = evaluate(&cases, 5, |_| vec!["x".into(), "y".into()]);
        assert_eq!(unrelated.recall_at_k, 0.0);
        assert_eq!(unrelated.mrr, 0.0);
        assert_eq!(unrelated.ndcg_at_k, 0.0);
    }

    /// MRR 应当区分"排第一"与"排第三"
    #[test]
    fn mrr_reflects_the_rank_of_the_first_hit() {
        let cases = vec![EvalCase {
            query: "q".into(),
            relevant: vec!["a".into()],
        }];
        let first = evaluate(&cases, 5, |_| vec!["a".into()]);
        let third = evaluate(&cases, 5, |_| vec!["x".into(), "y".into(), "a".into()]);
        assert!((first.mrr - 1.0).abs() < 1e-9);
        assert!((third.mrr - 1.0 / 3.0).abs() < 1e-9);
    }

    /// 回归底线：真实管线在这份语料上的表现不得退化。
    ///
    /// 这些数字是**从当前行为实测出来的**，不是质量目标——它只保证
    /// "不会悄悄变差"。等线上累积了真实标注（`recall_eval.jsonl`）之后，
    /// 才谈得上把目标定在文献意义上的合理区间。
    #[test]
    fn pipeline_quality_floor() {
        let metrics = evaluate(&corpus(), 5, retrieve_with_pipeline);

        assert!(
            metrics.recall_at_k >= 0.75,
            "召回率退化：{}",
            metrics.describe(5)
        );
        assert!(metrics.mrr >= 0.75, "排序质量退化：{}", metrics.describe(5));
        assert!(
            metrics.ndcg_at_k >= 0.75,
            "nDCG 退化：{}",
            metrics.describe(5)
        );
    }

    /// 消融：**没有查询向量缓存时，真正干活的只有 BM25 一路。**
    ///
    /// 设计书 §5.2d 指出 `vector_weight = 0.7` 实际作用在一个空列表上：
    /// 查询向量缓存以原始查询字符串为 key，而线上几乎不会有两条完全相同的
    /// 消息，所以第一次调用必然 miss。这条测试把那个事实钉住——
    /// 把权重全给向量，一条候选也拿不到。
    ///
    /// 真正的修法（查询规范化 + 异步补嵌入 + 话题缓存）是设计书 §5.3.3，
    /// 尚未实现；在那之前这条测试会一直亮着，提醒"双路检索"名不副实。
    #[test]
    fn without_a_cached_query_embedding_only_the_lexical_path_returns_candidates() {
        let lexical_only = evaluate(&corpus(), 5, |query| retrieve_with(query, 0.0, 0.7));
        let vector_only = evaluate(&corpus(), 5, |query| retrieve_with(query, 0.7, 0.0));

        assert!(
            lexical_only.recall_at_k > 0.0,
            "BM25 一路应当能拿到候选：{}",
            lexical_only.describe(5)
        );
        assert_eq!(
            vector_only.recall_at_k, 0.0,
            "没有查询向量时，向量权重再大也拿不到候选（这正是当前的实际状态）"
        );
    }
}
