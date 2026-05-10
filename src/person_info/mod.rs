//! 人物档案系统

mod store;

pub use store::*;

use tracing::{debug, info};

pub fn register_person(user_id: u64) {
    if user_id == 0 { return; }
    let mut s = load_store();
    let now = crate::util::now_secs();
    let p = s.profiles.entry(user_id).or_insert_with(|| { info!(user_id, "person_info: new person"); PersonProfile { user_id, know_since: now, ..Default::default() } });
    p.know_times += 1;
    p.last_know = now;
    save_store(&s);
}

pub fn get_profile(user_id: u64) -> Option<PersonProfile> { load_store().profiles.get(&user_id).cloned() }

pub fn set_name(user_id: u64, name: &str, reason: &str) {
    let mut s = load_store();
    if let Some(p) = s.profiles.get_mut(&user_id) { p.person_name = name.into(); p.name_reason = reason.into(); info!(user_id, name, "person_info: name set"); save_store(&s); }
}

pub fn add_memory_point(user_id: u64, point: &str) {
    let mut s = load_store();
    if let Some(p) = s.profiles.get_mut(&user_id) {
        if !p.memory_points.contains(&point.to_string()) { p.memory_points.push(point.into()); if p.memory_points.len() > 20 { p.memory_points.remove(0); } debug!(user_id, point, "person_info: memory point added"); save_store(&s); }
    }
}

pub fn set_group_nickname(user_id: u64, group_id: u64, nickname: &str) {
    let mut s = load_store();
    if let Some(p) = s.profiles.get_mut(&user_id) { p.group_nicknames.insert(group_id, nickname.into()); save_store(&s); }
}

pub fn get_person_context(user_id: u64) -> String {
    match load_store().profiles.get(&user_id) {
        Some(p) => {
            let mut lines = Vec::new();
            if !p.person_name.is_empty() { lines.push(format!("称呼：{}", p.person_name)); }
            if p.know_times > 20 { lines.push("关系：很熟悉的老朋友".into()); } else if p.know_times > 10 { lines.push("关系：已经比较熟悉".into()); } else if p.know_times > 3 { lines.push("关系：认识的人".into()); }
            if !p.memory_points.is_empty() { lines.push(format!("已知信息：{}", p.memory_points.iter().take(5).cloned().collect::<Vec<_>>().join("；"))); }
            if lines.is_empty() { String::new() } else { format!("# 关于这个人\n{}", lines.join("\n")) }
        }
        None => String::new(),
    }
}

pub fn get_group_members_context(group_id: u64, user_ids: &[u64]) -> String {
    let s = load_store();
    let mut lines = Vec::new();
    for &uid in user_ids {
        if let Some(p) = s.profiles.get(&uid) {
            let name = if !p.person_name.is_empty() { p.person_name.clone() } else if let Some(n) = p.group_nicknames.get(&group_id) { n.clone() } else { continue; };
            lines.push(format!("[{}] {} (认识{}次)", uid, name, p.know_times));
        }
    }
    if lines.is_empty() { String::new() } else { format!("# 群成员信息\n{}", lines.join("\n")) }
}
