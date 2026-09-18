//! Turn 契约的影子观测：读取与汇总
//!
//! 观测数据用来回答一个问题：**如果现在就把「文本即发言」换成严格的
//! `Turn` tagged union，有多少轮会变成 schema 失败？**
//!
//! 这个问题不能靠推理回答——设计书 §13.5 自己指出，`guard.rs` 那 341 行
//! 在一个"模型工具调用能力不可靠"的世界里是理性的成本，而在一个"能力可靠"
//! 的世界里是纯粹的债。哪一边为真由这个项目的实际运行环境决定，
//! 因此先量出来。

use crate::ai::TurnShape;
use crate::db::{TurnShadowStat, db};

/// 影子观测的汇总
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShadowReport {
    /// 参与契约统计的轮数（不含上游故障）
    pub rounds: u64,
    /// 其中在严格 Turn 契约下会被判为无效的轮数
    pub invalid: u64,
    /// 每个形态的 (形态, 总数, 其中无效)
    pub by_shape: Vec<(String, u64, u64)>,
}

impl ShadowReport {
    /// 严格契约下的失败率（0.0~1.0）；没有样本时返回 0
    pub fn failure_rate(&self) -> f64 {
        if self.rounds == 0 {
            return 0.0;
        }
        self.invalid as f64 / self.rounds as f64
    }

    /// 人类可读的一行，用于日志
    pub fn summary_line(&self) -> String {
        format!(
            "{} 轮样本，严格 Turn 契约下 {} 轮会失败（{:.1}%）",
            self.rounds,
            self.invalid,
            self.failure_rate() * 100.0
        )
    }

    pub fn to_json(&self) -> serde_json::Value {
        let by_shape: Vec<serde_json::Value> = self
            .by_shape
            .iter()
            .map(|(shape, total, invalid)| {
                serde_json::json!({
                    "shape": shape,
                    "total": total,
                    "invalid_under_turn": invalid,
                })
            })
            .collect();
        serde_json::json!({
            "window_rounds": self.rounds,
            "would_fail_rounds": self.invalid,
            "failure_rate": format!("{:.1}%", self.failure_rate() * 100.0),
            "by_shape": by_shape,
        })
    }
}

/// 把数据库的原始统计折成报告
///
/// 上游故障不参与分母：那是传输问题，不是"模型没遵守 schema"。
fn fold(stats: Vec<TurnShadowStat>) -> ShadowReport {
    let mut report = ShadowReport::default();
    for stat in stats {
        let Some(shape) = shape_from_label(&stat.shape) else {
            // 未知标签（例如旧版本写入的）只进明细，不进分母
            report.by_shape.push((stat.shape, stat.total, stat.invalid));
            continue;
        };
        if !shape.counts_toward_contract() {
            report.by_shape.push((stat.shape, stat.total, stat.invalid));
            continue;
        }
        report.rounds += stat.total;
        report.invalid += stat.invalid;
        report.by_shape.push((stat.shape, stat.total, stat.invalid));
    }
    report
}

fn shape_from_label(label: &str) -> Option<TurnShape> {
    [
        TurnShape::PlainText,
        TurnShape::EmptyResponse,
        TurnShape::LeakedFinishText,
        TurnShape::LeakedToolText,
        TurnShape::ToolCallClean,
        TurnShape::ToolCallWithNarration,
        TurnShape::ToolCallMultiple,
        TurnShape::RoundsExhausted,
        TurnShape::UpstreamFailed,
    ]
    .into_iter()
    .find(|shape| shape.as_str() == label)
}

/// 取最近 `window_secs` 秒的影子观测汇总
pub fn report(window_secs: u64) -> ShadowReport {
    let since = crate::util::now_secs().saturating_sub(window_secs) as i64;
    match db().turn_shadow_summary(since) {
        Ok(stats) => fold(stats),
        Err(error) => {
            tracing::warn!(%error, "turn_shadow: 读取汇总失败");
            ShadowReport::default()
        }
    }
}

/// 裁剪超过 `retain_secs` 的观测记录
pub fn prune(retain_secs: u64) -> usize {
    let before = crate::util::now_secs().saturating_sub(retain_secs) as i64;
    db().prune_turn_shadow(before).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_failures_do_not_count_as_contract_failures() {
        let report = fold(vec![
            TurnShadowStat {
                shape: "tool_call_clean".to_string(),
                total: 8,
                invalid: 0,
            },
            TurnShadowStat {
                shape: "plain_text".to_string(),
                total: 2,
                invalid: 2,
            },
            TurnShadowStat {
                shape: "upstream_failed".to_string(),
                total: 100,
                invalid: 100,
            },
        ]);

        assert_eq!(report.rounds, 10, "上游故障不该进分母");
        assert_eq!(report.invalid, 2);
        assert!((report.failure_rate() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn empty_report_has_zero_rate_and_a_readable_line() {
        let report = ShadowReport::default();
        assert_eq!(report.failure_rate(), 0.0);
        assert!(report.summary_line().contains("0 轮样本"));
    }

    /// 未知标签只进明细，不污染比率
    #[test]
    fn unknown_labels_stay_out_of_the_ratio() {
        let report = fold(vec![TurnShadowStat {
            shape: "some_future_shape".to_string(),
            total: 5,
            invalid: 5,
        }]);
        assert_eq!(report.rounds, 0);
        assert_eq!(report.failure_rate(), 0.0);
        assert_eq!(report.by_shape.len(), 1, "明细仍要保留，便于发现版本漂移");
    }
}
