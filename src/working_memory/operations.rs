use crate::db::{DeleteAtOutcome, WORKING_MEMORY_KEEP as MAX_ENTRIES_PER_GROUP, WorkingMemoryRow};

/// 一条工作记忆
///
/// 保留这个类型是因为归档层（`archive::archive_working_memory`）按它落库，
/// 而从状态库读出来的是 [`WorkingMemoryRow`]——两者的转换只有一处。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub user_id: u64,
    pub content: String,
    pub timestamp: u64,
    pub bot_replied: bool,
}

impl From<WorkingMemoryRow> for Entry {
    fn from(row: WorkingMemoryRow) -> Self {
        Self {
            user_id: row.user_id,
            content: row.content,
            timestamp: row.created_at.max(0) as u64,
            bot_replied: row.bot_replied,
        }
    }
}

/// 删除某个群的一条工作记忆的结果
///
/// 三种结果都是后台删除的正常结局（群没有记录、下标越界、删掉了），
/// 因此用枚举而不是 `Result<(), String>`：调用方能穷尽处理，
/// 也不会有"错误消息即契约"的问题。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeleteEntryOutcome {
    Removed,
    GroupNotFound,
    IndexOutOfRange,
}

/// 删除某个群的第 `index` 条工作记忆
pub(crate) fn delete_entry_at(group_id: u64, index: usize) -> DeleteEntryOutcome {
    let db = crate::db::db();
    match db.working_memory_group_exists(group_id) {
        Ok(true) => {}
        Ok(false) => return DeleteEntryOutcome::GroupNotFound,
        Err(error) => {
            tracing::warn!(%error, group_id, "working_memory: 查询群失败");
            return DeleteEntryOutcome::GroupNotFound;
        }
    }
    match db.working_memory_delete_at(group_id, index) {
        Ok(DeleteAtOutcome::Removed) => DeleteEntryOutcome::Removed,
        Ok(DeleteAtOutcome::OutOfRange) => DeleteEntryOutcome::IndexOutOfRange,
        Err(error) => {
            tracing::warn!(%error, group_id, index, "working_memory: 删除失败");
            DeleteEntryOutcome::IndexOutOfRange
        }
    }
}

/// 记录一条消息（无论是否回复），返回该条目的**稳定 id**
///
/// 返回 id 而不是时间戳：id 是身份，时间戳不是——同一秒内的多条消息
/// 用时间戳无法区分（旧实现用时间戳回填图片描述，因此会认错条目）。
///
/// 存储前剥离 Unicode emoji，防止污染记忆。
pub(crate) fn record(group_id: u64, user_id: u64, content: &str, bot_replied: bool) -> u64 {
    let cleaned = crate::emoji::strip_emoji(content);
    match crate::db::db().working_memory_push(
        group_id,
        user_id,
        &cleaned,
        bot_replied,
        MAX_ENTRIES_PER_GROUP,
    ) {
        Ok(id) => id.max(0) as u64,
        Err(error) => {
            tracing::warn!(%error, group_id, user_id, "working_memory: 写入失败");
            0
        }
    }
}

/// 标记某用户最近的消息为已回复
pub(crate) fn mark_replied(group_id: u64, user_id: u64) {
    if let Err(error) = crate::db::db().working_memory_mark_replied(group_id, user_id) {
        tracing::warn!(%error, group_id, user_id, "working_memory: 标记已回复失败");
    }
}

/// 记录机器人自己的回复到工作记忆（让 AI 知道之前说过什么）
pub(crate) fn record_bot_reply(group_id: u64, content: &str) {
    let self_qq = crate::config::get().self_qq;
    if self_qq == 0 {
        return;
    }
    let cleaned = crate::emoji::strip_emoji(content);
    if let Err(error) = crate::db::db().working_memory_push(
        group_id,
        self_qq,
        &cleaned,
        true,
        MAX_ENTRIES_PER_GROUP,
    ) {
        tracing::warn!(%error, group_id, "working_memory: 写入自己的回复失败");
    }
}

/// 获取指定时间戳之后的群聊消息（按记录顺序）
pub(crate) fn get_since(group_id: u64, since_timestamp: u64, max_count: usize) -> Vec<Entry> {
    match crate::db::db().working_memory_since(group_id, since_timestamp as i64, max_count) {
        Ok(rows) => rows.into_iter().map(Entry::from).collect(),
        Err(error) => {
            tracing::warn!(%error, group_id, "working_memory: 读取失败");
            Vec::new()
        }
    }
}

/// 清理过期的工作记忆（先归档，再删除）
pub(crate) fn cleanup(max_age_secs: u64) {
    let cutoff = crate::util::now_secs().saturating_sub(max_age_secs) as i64;
    let db = crate::db::db();

    let expired = match db.working_memory_expired(cutoff) {
        Ok(expired) => expired,
        Err(error) => {
            tracing::warn!(%error, "working_memory: 查询过期条目失败");
            return;
        }
    };
    if expired.is_empty() {
        return;
    }

    // 先归档再删除：反过来会丢数据
    let ids: Vec<i64> = expired.iter().map(|(_, row)| row.id).collect();
    let to_archive: Vec<(u64, Entry)> = expired
        .into_iter()
        .map(|(group_id, row)| (group_id, Entry::from(row)))
        .collect();
    crate::archive::archive_working_memory(to_archive);

    if let Err(error) = db.working_memory_delete_ids(&ids) {
        tracing::warn!(%error, count = ids.len(), "working_memory: 删除过期条目失败");
    }
}

/// 用精确的条目 id 把 [图片] 替换为实际图片描述
///
/// `entry_ids` 来自 [`record`] 的返回值。旧实现用写入时间戳匹配，
/// 同一秒内的多条消息会互相认错。
pub(crate) fn update_image_content(
    group_id: u64,
    user_id: u64,
    image_descriptions: &[String],
    entry_ids: &[u64],
) {
    if group_id == 0 || image_descriptions.is_empty() || entry_ids.is_empty() {
        return;
    }
    let db = crate::db::db();
    let entries = match db.working_memory_of_group(group_id) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(%error, group_id, "working_memory: 读取失败，跳过图片描述回填");
            return;
        }
    };

    let mut desc_idx = 0usize;
    for &entry_id in entry_ids {
        if desc_idx >= image_descriptions.len() {
            break;
        }
        let entry_id = entry_id as i64;
        let Some(row) = entries
            .iter()
            .find(|row| row.id == entry_id && row.user_id == user_id)
        else {
            continue;
        };
        let mut content = row.content.clone();
        while content.contains("[图片]") && desc_idx < image_descriptions.len() {
            content = content.replacen(
                "[图片]",
                &format!("[图片: {}]", image_descriptions[desc_idx]),
                1,
            );
            desc_idx += 1;
        }
        if content != row.content
            && let Err(error) = db.working_memory_set_content(entry_id, &content)
        {
            tracing::warn!(%error, entry_id, "working_memory: 回填图片描述失败");
        }
    }
}

/// 返回有工作记忆的群数量（用于启动日志）
pub(crate) fn group_count() -> usize {
    crate::db::db().working_memory_group_count().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 条目身份是 id，不是时间戳：同一秒内写入的两条必须能分别回填
    #[test]
    fn entry_identity_is_the_id_not_the_timestamp() {
        let db = crate::db::db();
        let group = 991_001;

        let first = db
            .working_memory_push(group, 11, "看这个[图片]", false, MAX_ENTRIES_PER_GROUP)
            .expect("写入");
        let second = db
            .working_memory_push(group, 12, "还有这个[图片]", false, MAX_ENTRIES_PER_GROUP)
            .expect("写入");
        assert_ne!(first, second, "两条必须是不同的身份");

        update_image_content(group, 12, &["一只猫".to_string()], &[second as u64]);

        let rows = db.working_memory_of_group(group).expect("读取");
        let second_row = rows.iter().find(|row| row.id == second).expect("第二条");
        let first_row = rows.iter().find(|row| row.id == first).expect("第一条");
        assert!(
            second_row.content.contains("一只猫"),
            "第二条应被回填：{}",
            second_row.content
        );
        assert_eq!(
            first_row.content, "看这个[图片]",
            "第一条不该被认错（同一秒内写入时旧实现会认错）"
        );

        db.working_memory_delete_ids(&[first, second])
            .expect("清理");
    }

    /// 每群裁剪到上限，且保留的是**最新**的那些
    #[test]
    fn per_group_trim_keeps_the_newest() {
        let db = crate::db::db();
        let group = 991_002;
        let keep = 3usize;

        let mut ids = Vec::new();
        for i in 0..6 {
            ids.push(
                db.working_memory_push(group, 11, &format!("第{i}条"), false, keep)
                    .expect("写入"),
            );
        }
        let rows = db.working_memory_of_group(group).expect("读取");
        assert_eq!(rows.len(), keep, "每个群最多保留 {keep} 条");
        assert_eq!(rows[0].content, "第3条", "保留的应该是最新的几条");

        db.working_memory_delete_ids(&[rows[0].id, rows[1].id, rows[2].id])
            .expect("清理");
        assert!(ids.len() == 6);
    }
}
