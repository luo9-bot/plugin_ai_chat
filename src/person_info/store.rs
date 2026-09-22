//! 人物档案数据结构与持久化
//!
//! 档案按用户一行存在状态库里：读取是主键查询，写入是单行 UPSERT。
//! 先前它们是 `person_info.json` 里的一整张 map，配一个进程内缓存——
//! 于是"读一个人"要 clone 全表，"写一个人"要重写全文件，而
//! `get_display_name` 在每条消息上都用。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::db::PerUserState;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct PersonProfile {
    pub user_id: u64,
    pub person_name: String,
    pub name_reason: String,
    pub know_times: u32,
    pub know_since: u64,
    pub last_know: u64,
    pub memory_points: Vec<String>,
    pub group_nicknames: HashMap<u64, String>,
}

/// 读一个用户的档案
pub(crate) fn load_profile(user_id: u64) -> Option<PersonProfile> {
    match crate::db::db().per_user_state(PerUserState::Person, user_id) {
        Ok(Some(json)) => serde_json::from_str(&json).ok(),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, user_id, "person_info: 读取失败");
            None
        }
    }
}

/// 写一个用户的档案
pub(crate) fn save_profile(profile: &PersonProfile) {
    let json = match serde_json::to_string(profile) {
        Ok(json) => json,
        Err(error) => {
            tracing::warn!(%error, user_id = profile.user_id, "person_info: 序列化失败，未写入");
            return;
        }
    };
    if let Err(error) =
        crate::db::db().set_per_user_state(PerUserState::Person, profile.user_id, &json)
    {
        tracing::warn!(%error, user_id = profile.user_id, "person_info: 写库失败");
    }
}

/// 单用户档案的读改写
///
/// 抽出这个组合是因为"读一行 → 改 → 写一行"是本模块唯一的写形态：
/// 缺记录时用 `PersonProfile::default()` 起手，而不是凭空造一个带
/// 时间戳的默认值（那是调用方的事）。
pub(crate) fn update_profile(user_id: u64, mutate: impl FnOnce(&mut PersonProfile)) {
    let mut profile = load_profile(user_id).unwrap_or_else(|| PersonProfile {
        user_id,
        ..Default::default()
    });
    mutate(&mut profile);
    save_profile(&profile);
}
