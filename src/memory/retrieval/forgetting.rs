//! 无状态遗忘曲线
//!
//! 遗忘不是后台任务，而是查询时的纯函数（借鉴搜索引擎粗排的 timeliness 思路）：
//! `retention = 0.5^(elapsed / half_life)`，任何时刻崩溃重启都不需要重建衰减状态。
//! 半衰期随"被想起的次数"对数延长——检索即强化，回忆让记忆更牢固。
//!
//! 融合方式沿用 AIRI 式加权：`score × (w_sim + w_time × retention) / (w_sim + w_time)`。
//! 完全鲜活（retention=1）的分数不变，彻底淡忘的记忆被温和压向阈值之外。

use super::RetrievalResult;

/// 单条记忆参与遗忘计算所需的元数据
#[derive(Debug, Clone, Copy)]
pub struct MemoryMeta {
    /// 上次被想起的时间（unix 秒）
    pub last_accessed: u64,
    /// 被想起的次数（含写入时的第一次）
    pub access_count: u32,
    pub is_permanent: bool,
    pub is_important: bool,
}

/// 遗忘曲线配置
#[derive(Debug, Clone)]
pub struct ForgettingConfig {
    pub enabled: bool,
    /// 基础半衰期（秒）：普通记忆从"上次想起"到淡忘到一半的时间
    pub half_life_secs: f64,
    /// 时间分权重（1.2×相似 + 0.2×时间 中的时间项）
    pub time_weight: f64,
    /// 相似分权重
    pub similarity_weight: f64,
    /// 每次回忆对半衰期的对数增益
    pub reinforcement_gain: f64,
}

impl Default for ForgettingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            half_life_secs: 14.0 * 86400.0,
            time_weight: 0.2,
            similarity_weight: 1.2,
            reinforcement_gain: 0.5,
        }
    }
}

/// 无状态保留率：距上次想起 `elapsed` 秒后还记得多少（0~1]
pub fn retention(elapsed_secs: f64, half_life_secs: f64) -> f64 {
    if half_life_secs <= 0.0 || !half_life_secs.is_finite() {
        return 1.0;
    }
    0.5_f64.powf(elapsed_secs / half_life_secs)
}

/// 有效半衰期：回忆次数越多忘得越慢（对数强化）；重要记忆×4；永久记忆不衰减
pub fn effective_half_life(meta: &MemoryMeta, cfg: &ForgettingConfig) -> f64 {
    if meta.is_permanent {
        return f64::INFINITY;
    }
    let reinforcement = 1.0 + cfg.reinforcement_gain * (1.0 + meta.access_count as f64).ln();
    let importance_mult = if meta.is_important { 4.0 } else { 1.0 };
    cfg.half_life_secs * reinforcement * importance_mult
}

/// 把遗忘曲线应用到检索结果上（按 id 查元数据，查不到的不动）
pub fn apply(
    results: &mut [RetrievalResult],
    meta_of: impl Fn(&str) -> Option<MemoryMeta>,
    now: u64,
    cfg: &ForgettingConfig,
) {
    if !cfg.enabled {
        return;
    }
    let denom = cfg.similarity_weight + cfg.time_weight;
    if denom <= 0.0 {
        return;
    }
    for result in results.iter_mut() {
        let Some(meta) = meta_of(&result.id) else {
            continue;
        };
        let half = effective_half_life(&meta, cfg);
        if half.is_infinite() {
            continue;
        }
        let elapsed = now.saturating_sub(meta.last_accessed) as f64;
        let keep = retention(elapsed, half);
        result.score *= (cfg.similarity_weight + cfg.time_weight * keep) / denom;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ForgettingConfig {
        ForgettingConfig::default()
    }

    #[test]
    fn retention_follows_half_life() {
        // 一个半衰期后剩一半，两个后剩四分之一
        assert!((retention(14.0 * 86400.0, 14.0 * 86400.0) - 0.5).abs() < 1e-9);
        assert!((retention(28.0 * 86400.0, 14.0 * 86400.0) - 0.25).abs() < 1e-9);
        // 刚想起的记忆近乎完整
        assert!(retention(0.0, 14.0 * 86400.0) > 0.999);
        // 无效半衰期视为不衰减
        assert_eq!(retention(1e9, 0.0), 1.0);
        assert_eq!(retention(1e9, f64::INFINITY), 1.0);
    }

    #[test]
    fn reinforcement_extends_half_life() {
        let c = cfg();
        let once = MemoryMeta {
            last_accessed: 0,
            access_count: 1,
            is_permanent: false,
            is_important: false,
        };
        let often = MemoryMeta {
            access_count: 10,
            ..once
        };
        let h_once = effective_half_life(&once, &c);
        let h_often = effective_half_life(&often, &c);
        assert!(h_often > h_once, "常被想起的记忆半衰期应更长");
        // 重要记忆 4 倍
        let important = MemoryMeta {
            is_important: true,
            ..once
        };
        assert!((effective_half_life(&important, &c) / h_once - 4.0).abs() < 1e-9);
        // 永久记忆不衰减
        let permanent = MemoryMeta {
            is_permanent: true,
            ..once
        };
        assert!(effective_half_life(&permanent, &c).is_infinite());
    }

    #[test]
    fn apply_decays_stale_and_spares_fresh() {
        let c = cfg();
        let now = 100_000_000_u64;
        let day = 86400_u64;
        let fresh = RetrievalResult {
            id: "fresh".into(),
            content: String::new(),
            score: 1.0,
        };
        let stale = RetrievalResult {
            id: "stale".into(),
            content: String::new(),
            score: 1.0,
        };
        let mut results = vec![fresh, stale];
        apply(
            &mut results,
            |id| match id {
                "fresh" => Some(MemoryMeta {
                    last_accessed: now,
                    access_count: 1,
                    is_permanent: false,
                    is_important: false,
                }),
                "stale" => Some(MemoryMeta {
                    last_accessed: now - 56 * day, // 四个半衰期
                    access_count: 1,
                    is_permanent: false,
                    is_important: false,
                }),
                _ => None,
            },
            now,
            &c,
        );
        let fresh_score = results.iter().find(|r| r.id == "fresh").unwrap().score;
        let stale_score = results.iter().find(|r| r.id == "stale").unwrap().score;
        // 鲜活的分数不动（1.2 + 0.2*1 = 1.4，缩放系数为 1）
        assert!((fresh_score - 1.0).abs() < 1e-9);
        // 陈旧的被压低但不清零（时间项只占 1/7 权重）
        assert!(stale_score < 1.0);
        assert!(stale_score > 1.2 / 1.4 - 1e-9);
    }
}
