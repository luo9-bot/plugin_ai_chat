//! 回复效果数据结构和持久化

use crate::util::MutexExt;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) enum EffectStatus {
    Pending,
    Finalized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FollowupMessage {
    pub user_id: u64,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ReplyEffectRecord {
    pub reply_text: String,
    pub target_user: u64,
    pub group_id: u64,
    pub sent_at: u64,
    pub followups: Vec<FollowupMessage>,
    pub asi_score: Option<f64>,
    pub status: EffectStatus,
    /// 训练留档 replies.db 的行 id——评分定稿时把 reward 写回（旧记录无此字段）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_reply_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct EffectStore {
    pub records: Vec<ReplyEffectRecord>,
}

pub(crate) static STORE: Mutex<Option<EffectStore>> = Mutex::new(None);

pub(crate) fn store_path() -> std::path::PathBuf {
    crate::config::data_dir().join("reply_effects.json")
}

pub(crate) fn load_store() -> EffectStore {
    let mut guard = STORE.lock_recover();
    if guard.is_none() {
        *guard = Some(crate::util::load_json(&store_path()));
    }
    guard.clone().unwrap_or_default()
}

pub(crate) fn save_store(store: &EffectStore) {
    {
        let mut guard = STORE.lock_recover();
        *guard = Some(store.clone());
        // 锁在这里释放：磁盘延迟不该决定锁的持有时间
    }
    if let Err(error) = crate::util::save_json(&store_path(), store) {
        tracing::warn!(error = %error, path = %store_path().display(), "reply_effect: 持久化失败");
    }
}

pub(crate) const OBSERVATION_WINDOW: u64 = 600;
pub(crate) const MAX_FOLLOWUPS: usize = 10;
pub(crate) const MAX_ACTIVE_RECORDS: usize = 20;
