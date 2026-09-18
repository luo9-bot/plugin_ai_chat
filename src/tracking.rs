//! Token 用量 + Prompt 统计追踪
//!
//! 用量是**追加流**，不是"读 10000 条 + 改 + 写回"的文档：旧实现每次模型
//! 调用后都重写整份 `api_usage.json`（上限 10,000 条），而模型调用本身
//! 是这条管线上最频繁的事情之一。现在明细进 `api_usage` 表（一条 INSERT），
//! 终身累计进 `api_usage_total` 表（一条 UPSERT）——两者都是 O(1)。
//!
//! 累计与明细刻意分表：明细会被裁剪到 10,000 条，而"一共花了多少 token"
//! 不该因此缩水。

use crate::db::ApiUsage;

/// 记录一次调用
pub fn record_call(
    prompt_name: &str,
    model: &str,
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    cache_hit: u32,
    cache_miss: u32,
) {
    let usage = ApiUsage {
        ts: crate::util::now_secs() as i64,
        model: model.to_string(),
        prompt_name: prompt_name.to_string(),
        prompt_tokens,
        completion_tokens,
        total_tokens,
        cache_hit,
        cache_miss,
    };
    if let Err(error) = crate::db::db().record_api_usage(&usage, crate::db::API_USAGE_KEEP) {
        tracing::warn!(%error, "tracking: 用量写入失败");
    }
}

/// 统计摘要（供 `/api/analytics`）
///
/// 返回的 JSON 形状与前端既有约定一致，因此这里的改动对界面是透明的。
pub fn summary() -> serde_json::Value {
    let db = crate::db::db();

    let totals = match db.api_usage_totals() {
        Ok(totals) => totals,
        Err(error) => {
            tracing::warn!(%error, "tracking: 读取累计用量失败");
            Default::default()
        }
    };
    let by_prompt = db.api_usage_by_prompt().unwrap_or_default();
    let recent = db.api_usage_recent(50).unwrap_or_default();

    let prompts: Vec<serde_json::Value> = by_prompt
        .iter()
        .map(|entry| {
            serde_json::json!({
                "name": entry.prompt_name,
                "calls": entry.calls,
                "prompt_tokens": entry.prompt_tokens,
                "completion_tokens": entry.completion_tokens,
                "total_tokens": entry.total_tokens,
                "cache_hit": entry.cache_hit,
                "cache_miss": entry.cache_miss,
                "avg_total": if entry.calls > 0 { entry.total_tokens / entry.calls } else { 0 },
            })
        })
        .collect();

    let recent: Vec<serde_json::Value> = recent
        .iter()
        .map(|row| {
            serde_json::json!({
                "time": row.ts,
                "model": row.model,
                "prompt": row.prompt_name,
                "prompt_tokens": row.prompt_tokens,
                "completion_tokens": row.completion_tokens,
                "total_tokens": row.total_tokens,
                "cache_hit": row.cache_hit,
                "cache_miss": row.cache_miss,
            })
        })
        .collect();

    let cache_total = totals.cache_hit + totals.cache_miss;
    serde_json::json!({
        "total_calls": totals.calls,
        "total_prompt_tokens": totals.prompt_tokens,
        "total_completion_tokens": totals.completion_tokens,
        "total_tokens": totals.prompt_tokens + totals.completion_tokens,
        "total_cache_hit": totals.cache_hit,
        "total_cache_miss": totals.cache_miss,
        "cache_hit_ratio": if cache_total > 0 {
            format!("{:.1}%", totals.cache_hit as f64 / cache_total as f64 * 100.0)
        } else {
            "0%".to_string()
        },
        "by_prompt": prompts,
        "recent": recent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 追加流：写进去就能读出来，且累计与明细一致
    ///
    /// 断言全部基于**增量**：状态库落在真实 data 目录，
    /// `api_usage_total` 是终身累计，会跨测试运行累积。
    #[test]
    fn recorded_calls_show_up_in_both_detail_and_totals() {
        let db = crate::db::db();
        let marker = "tracking_test_prompt";

        let entry_calls = |db: &crate::db::Db| -> u64 {
            db.api_usage_by_prompt()
                .unwrap_or_default()
                .iter()
                .find(|entry| entry.prompt_name == marker)
                .map(|entry| entry.calls)
                .unwrap_or(0)
        };
        let entry_tokens = |db: &crate::db::Db| -> u64 {
            db.api_usage_by_prompt()
                .unwrap_or_default()
                .iter()
                .find(|entry| entry.prompt_name == marker)
                .map(|entry| entry.total_tokens)
                .unwrap_or(0)
        };

        let before = db.api_usage_totals().expect("累计");
        let before_entry_calls = entry_calls(db);
        let before_entry_tokens = entry_tokens(db);

        record_call(marker, "test-model", 10, 5, 15, 3, 7);
        record_call(marker, "test-model", 20, 10, 30, 1, 9);

        let after = db.api_usage_totals().expect("累计");
        assert_eq!(after.calls, before.calls + 2, "累计调用数应 +2");
        assert_eq!(after.prompt_tokens, before.prompt_tokens + 30);
        assert_eq!(after.completion_tokens, before.completion_tokens + 15);
        assert_eq!(after.cache_hit, before.cache_hit + 4);
        assert_eq!(after.cache_miss, before.cache_miss + 16);

        assert_eq!(
            entry_calls(db) - before_entry_calls,
            2,
            "按 prompt 分组的调用数应 +2"
        );
        assert_eq!(
            entry_tokens(db) - before_entry_tokens,
            45,
            "按 prompt 分组的 token 数应 +45"
        );

        let recent = db.api_usage_recent(10).expect("明细");
        assert!(
            recent.iter().any(|row| row.prompt_name == marker),
            "明细里应能看到刚写入的调用"
        );
    }

    /// 摘要的形状是前端契约：字段名与类型都不能变
    #[test]
    fn summary_keeps_the_frontend_contract() {
        let summary = summary();
        for key in [
            "total_calls",
            "total_prompt_tokens",
            "total_completion_tokens",
            "total_tokens",
            "total_cache_hit",
            "total_cache_miss",
            "cache_hit_ratio",
            "by_prompt",
            "recent",
        ] {
            assert!(summary.get(key).is_some(), "摘要缺少字段 {key}");
        }
        assert!(summary["by_prompt"].is_array());
        assert!(summary["recent"].is_array());
    }
}
