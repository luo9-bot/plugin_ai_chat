//! Token 用量 + Prompt 统计追踪
//!
//! 持久化到 data_dir/api_usage.json

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

/// 单次 API 调用记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCallRecord {
    pub timestamp: u64,
    pub model: String,
    pub prompt_name: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub prompt_cache_hit: u32,
    pub prompt_cache_miss: u32,
}

/// 聚合统计
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregatedStats {
    pub total_calls: u64,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_cache_hit: u64,
    pub total_cache_miss: u64,
    pub by_prompt: HashMap<String, PromptStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptStat {
    pub calls: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub cache_hit: u64,
    pub cache_miss: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageStore {
    pub records: Vec<ApiCallRecord>,
    pub aggregated: AggregatedStats,
}

impl UsageStore {
    fn path() -> std::path::PathBuf {
        crate::config::data_dir().join("api_usage.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        match fs::read_to_string(&path) {
            Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self) {
        let path = Self::path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            fs::write(path, json).ok();
        }
    }

    pub fn record_call(
        prompt_name: &str,
        model: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
        total_tokens: u32,
        cache_hit: u32,
        cache_miss: u32,
    ) {
        let mut store = Self::load();
        let now = crate::util::now_secs();

        // 添加记录（最多保留 10000 条防无限增长）
        store.records.push(ApiCallRecord {
            timestamp: now,
            model: model.to_string(),
            prompt_name: prompt_name.to_string(),
            prompt_tokens,
            completion_tokens,
            total_tokens,
            prompt_cache_hit: cache_hit,
            prompt_cache_miss: cache_miss,
        });
        if store.records.len() > 10000 {
            store.records.drain(0..store.records.len() - 10000);
        }

        // 更新聚合
        let a = &mut store.aggregated;
        a.total_calls += 1;
        a.total_prompt_tokens += prompt_tokens as u64;
        a.total_completion_tokens += completion_tokens as u64;
        a.total_cache_hit += cache_hit as u64;
        a.total_cache_miss += cache_miss as u64;

        let ps = a.by_prompt.entry(prompt_name.to_string()).or_default();
        ps.calls += 1;
        ps.prompt_tokens += prompt_tokens as u64;
        ps.completion_tokens += completion_tokens as u64;
        ps.total_tokens += total_tokens as u64;
        ps.cache_hit += cache_hit as u64;
        ps.cache_miss += cache_miss as u64;

        store.save();
    }

    /// 获取统计摘要（用于 API）
    pub fn summary() -> serde_json::Value {
        let store = Self::load();
        let a = &store.aggregated;

        // 按 prompt 排序并格式化
        let mut sorted: Vec<(&String, &PromptStat)> = a.by_prompt.iter().collect();
        sorted.sort_by(|a, b| b.1.total_tokens.cmp(&a.1.total_tokens));

        let prompts: Vec<serde_json::Value> = sorted.iter().map(|(name, s)| {
            serde_json::json!({
                "name": name,
                "calls": s.calls,
                "prompt_tokens": s.prompt_tokens,
                "completion_tokens": s.completion_tokens,
                "total_tokens": s.total_tokens,
                "cache_hit": s.cache_hit,
                "cache_miss": s.cache_miss,
                "avg_total": if s.calls > 0 { s.total_tokens / s.calls as u64 } else { 0 },
            })
        }).collect();

        // 最近记录（取最后 50 条反向）
        let recent: Vec<serde_json::Value> = store.records.iter().rev().take(50).map(|r| {
            serde_json::json!({
                "time": r.timestamp,
                "model": r.model,
                "prompt": r.prompt_name,
                "prompt_tokens": r.prompt_tokens,
                "completion_tokens": r.completion_tokens,
                "total_tokens": r.total_tokens,
                "cache_hit": r.prompt_cache_hit,
                "cache_miss": r.prompt_cache_miss,
            })
        }).collect();

        serde_json::json!({
            "total_calls": a.total_calls,
            "total_prompt_tokens": a.total_prompt_tokens,
            "total_completion_tokens": a.total_completion_tokens,
            "total_tokens": a.total_prompt_tokens + a.total_completion_tokens,
            "total_cache_hit": a.total_cache_hit,
            "total_cache_miss": a.total_cache_miss,
            "cache_hit_ratio": if (a.total_cache_hit + a.total_cache_miss) > 0 {
                format!("{:.1}%", a.total_cache_hit as f64 / (a.total_cache_hit + a.total_cache_miss) as f64 * 100.0)
            } else { "0%".into() },
            "by_prompt": prompts,
            "recent": recent,
        })
    }
}
