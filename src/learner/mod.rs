//! 表达学习系统

mod store;
mod extract;

pub use store::*;
pub use extract::*;

pub fn get_expression_context(group_id: u64, max_count: usize) -> String {
    let s = load_store();
    let mut exprs: Vec<&ExpressionHabit> = s.expressions.iter().filter(|e| e.source_group == group_id || e.source_group == 0).collect();
    exprs.sort_by(|a, b| b.count.cmp(&a.count));
    let sel: Vec<&ExpressionHabit> = exprs.into_iter().take(max_count).collect();
    if sel.is_empty() { return String::new(); }
    let mut lines = vec!["# 表达习惯参考".to_string()];
    for e in &sel { lines.push(format!("- 当{}时，可以{}", e.situation, e.style)); }
    lines.join("\n")
}
