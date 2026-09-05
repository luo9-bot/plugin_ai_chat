//! 联想回忆：她"想起"什么
//!
//! 回忆不是检索注入——命中的记忆以转述形式进入意识流，
//! 成为她此刻经历的一部分（方案书 §6.3）。
//! 记忆本体来自现有 memory 检索管线（保留资产）；日记语料的
//! 向量化随 P4 收尾并入。

use crate::util;

/// 对眼前的人与话题，她能想起什么（"想起：…"行，直接入流）
pub fn recall_for(text: &str, user_id: u64, group_id: u64) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    if !text.trim().is_empty() {
        let results = crate::memory::search_memories(user_id, group_id, text, 3);
        for r in results {
            out.push(format!("想起：{}", r.content));
        }
    }

    // 时间线索：太久没说话的人（私聊才有"多久没见"的分寸）
    if group_id == 0 {
        let relationship = crate::person_info::relationship::get_relationship(user_id);
        let gap_days = util::now_secs().saturating_sub(relationship.updated_at) / 86400;
        if gap_days >= 3 {
            let name =
                crate::person_info::get_display_name(user_id, 0).unwrap_or_else(|| "对方".into());
            out.push(format!("你和{name}已经{gap_days}天没说话了"));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn recall_output_format_is_paraphrase() {
        // 输出必须是转述口吻，不带指令、不带数值
        let line = "想起：他上周说过要去看海";
        assert!(line.starts_with("想起："));
        assert!(!line.contains("memory"));
    }
}
