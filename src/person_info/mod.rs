//! 人物档案系统
//!
//! 追踪 bot 认识的每个人的信息。
//! 参考 MaiBot 的 person_info 架构。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{debug, info};

/// 人物档案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonProfile {
    /// 用户 QQ 号
    pub user_id: u64,
    /// bot 如何称呼 ta
    pub person_name: String,
    /// 为什么叫这个名字
    pub name_reason: String,
    /// 遇到次数
    pub know_times: u32,
    /// 首次见面时间 (unix timestamp)
    pub know_since: u64,
    /// 最后见面时间
    pub last_know: u64,
    /// 记忆点列表
    pub memory_points: Vec<String>,
    /// 群内昵称 (group_id -> nickname)
    pub group_nicknames: HashMap<u64, String>,
}

impl Default for PersonProfile {
    fn default() -> Self {
        Self {
            user_id: 0,
            person_name: String::new(),
            name_reason: String::new(),
            know_times: 0,
            know_since: 0,
            last_know: 0,
            memory_points: Vec::new(),
            group_nicknames: HashMap::new(),
        }
    }
}

/// 人物档案存储
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersonStore {
    pub profiles: HashMap<u64, PersonProfile>,
}

static STORE: Mutex<Option<PersonStore>> = Mutex::new(None);

fn store_path() -> std::path::PathBuf {
    crate::config::data_dir().join("person_info.json")
}

fn load_store() -> PersonStore {
    let mut guard = STORE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(crate::util::load_json(&store_path()));
    }
    guard.clone().unwrap_or_default()
}

fn save_store(store: &PersonStore) {
    let mut guard = STORE.lock().unwrap();
    *guard = Some(store.clone());
    crate::util::save_json(&store_path(), store);
}

/// 注册/更新人物（每次收到消息时调用）
pub fn register_person(user_id: u64) {
    if user_id == 0 {
        return;
    }

    let mut store = load_store();
    let now = crate::util::now_secs();

    let profile = store.profiles.entry(user_id).or_insert_with(|| {
        info!(user_id, "person_info: new person");
        PersonProfile {
            user_id,
            know_since: now,
            ..Default::default()
        }
    });

    profile.know_times += 1;
    profile.last_know = now;

    save_store(&store);
}

/// 获取人物档案
pub fn get_profile(user_id: u64) -> Option<PersonProfile> {
    let store = load_store();
    store.profiles.get(&user_id).cloned()
}

/// 设置人物名字
pub fn set_name(user_id: u64, name: &str, reason: &str) {
    let mut store = load_store();
    if let Some(profile) = store.profiles.get_mut(&user_id) {
        profile.person_name = name.to_string();
        profile.name_reason = reason.to_string();
        info!(user_id, name, reason, "person_info: name set");
        save_store(&store);
    }
}

/// 添加记忆点
pub fn add_memory_point(user_id: u64, point: &str) {
    let mut store = load_store();
    if let Some(profile) = store.profiles.get_mut(&user_id) {
        // 去重
        if !profile.memory_points.contains(&point.to_string()) {
            profile.memory_points.push(point.to_string());
            // 限制数量
            if profile.memory_points.len() > 20 {
                profile.memory_points.remove(0);
            }
            debug!(user_id, point, "person_info: memory point added");
            save_store(&store);
        }
    }
}

/// 设置群内昵称
pub fn set_group_nickname(user_id: u64, group_id: u64, nickname: &str) {
    let mut store = load_store();
    if let Some(profile) = store.profiles.get_mut(&user_id) {
        profile.group_nicknames.insert(group_id, nickname.to_string());
        save_store(&store);
    }
}

/// 获取人物上下文（注入到 prompt 中）
pub fn get_person_context(user_id: u64) -> String {
    let store = load_store();
    match store.profiles.get(&user_id) {
        Some(profile) => {
            let mut lines = Vec::new();

            if !profile.person_name.is_empty() {
                lines.push(format!("称呼：{}", profile.person_name));
            }

            if profile.know_times > 0 {
                lines.push(format!("认识次数：{}", profile.know_times));
            }

            if profile.know_times > 20 {
                lines.push("关系：很熟悉的老朋友".to_string());
            } else if profile.know_times > 10 {
                lines.push("关系：已经比较熟悉".to_string());
            } else if profile.know_times > 3 {
                lines.push("关系：认识的人".to_string());
            }

            if !profile.memory_points.is_empty() {
                let points = profile.memory_points.iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("；");
                lines.push(format!("已知信息：{}", points));
            }

            if lines.is_empty() {
                String::new()
            } else {
                format!("# 关于这个人\n{}", lines.join("\n"))
            }
        }
        None => String::new(),
    }
}

/// 获取群内所有人的简要信息
pub fn get_group_members_context(group_id: u64, user_ids: &[u64]) -> String {
    let store = load_store();
    let mut lines = Vec::new();

    for &uid in user_ids {
        if let Some(profile) = store.profiles.get(&uid) {
            let name = if !profile.person_name.is_empty() {
                profile.person_name.clone()
            } else if let Some(nick) = profile.group_nicknames.get(&group_id) {
                nick.clone()
            } else {
                continue; // 没有名字信息就跳过
            };
            lines.push(format!("[{}] {} (认识{}次)", uid, name, profile.know_times));
        }
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!("# 群成员信息\n{}", lines.join("\n"))
    }
}
