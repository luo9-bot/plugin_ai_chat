//! 表达学习系统

mod store;
mod extract;

pub use store::*;
pub use extract::*;

use tracing::debug;

/// 最少候选数才激活 LLM 选择
const MIN_CANDIDATES_FOR_LLM: usize = 10;

/// 获取表达习惯上下文
///
/// 如果候选数 >= 10，使用 LLM 子代理从候选中选择最合适的；
/// 否则直接取 top-N。
pub fn get_expression_context(group_id: u64, max_count: usize, chat_context: &str) -> String {
    let s = load_store();
    let candidates: Vec<&ExpressionHabit> = s.expressions.iter()
        .filter(|e| e.source_group == group_id || e.source_group == 0)
        .collect();

    if candidates.is_empty() {
        return String::new();
    }

    // 如果候选数足够，使用 LLM 选择
    if candidates.len() >= MIN_CANDIDATES_FOR_LLM && !chat_context.is_empty() {
        return select_expressions_with_llm(&candidates, chat_context, max_count);
    }

    // 否则直接取 top-N by count
    let mut sorted = candidates;
    sorted.sort_by(|a, b| b.count.cmp(&a.count));
    let sel: Vec<&ExpressionHabit> = sorted.into_iter().take(max_count).collect();
    format_expressions(&sel)
}

/// 使用 LLM 从候选中选择最合适的表达
fn select_expressions_with_llm(
    candidates: &[&ExpressionHabit],
    chat_context: &str,
    max_count: usize,
) -> String {
    // 构建候选列表
    let candidate_list: Vec<String> = candidates.iter()
        .enumerate()
        .map(|(i, e)| format!("{}. 当{}时，可以{}", i + 1, e.situation, e.style))
        .collect();

    let prompt = format!(
        "从以下表达习惯中，选择最适合当前对话上下文的 0-3 个。\n\n\
         当前对话上下文：\n{}\n\n\
         候选表达：\n{}\n\n\
         只返回选中的序号（逗号分隔），如果都不合适返回空。",
        chat_context,
        candidate_list.join("\n")
    );

    match crate::ai::analyze("", &prompt) {
        Ok(response) => {
            // 解析选中的序号
            let selected: Vec<&ExpressionHabit> = response
                .split([',', '，', ' '])
                .filter_map(|s| s.trim().parse::<usize>().ok())
                .filter(|&idx| idx > 0 && idx <= candidates.len())
                .map(|idx| candidates[idx - 1])
                .take(max_count)
                .collect();

            if selected.is_empty() {
                debug!("expression_selector: LLM selected none, falling back to top-N");
                let mut sorted: Vec<&ExpressionHabit> = candidates.to_vec();
                sorted.sort_by(|a, b| b.count.cmp(&a.count));
                let sel: Vec<&ExpressionHabit> = sorted.into_iter().take(max_count).collect();
                return format_expressions(&sel);
            }

            debug!(count = selected.len(), "expression_selector: LLM selected");
            format_expressions(&selected)
        }
        Err(e) => {
            debug!(error = %e, "expression_selector: LLM failed, falling back to top-N");
            let mut sorted: Vec<&ExpressionHabit> = candidates.to_vec();
            sorted.sort_by(|a, b| b.count.cmp(&a.count));
            let sel: Vec<&ExpressionHabit> = sorted.into_iter().take(max_count).collect();
            format_expressions(&sel)
        }
    }
}

/// 格式化表达列表为 prompt 文本
fn format_expressions(exprs: &[&ExpressionHabit]) -> String {
    if exprs.is_empty() {
        return String::new();
    }
    let mut lines = vec!["# 表达习惯参考".to_string()];
    for e in exprs {
        lines.push(format!("- 当{}时，可以{}", e.situation, e.style));
    }
    lines.join("\n")
}
