//! 人物档案：她眼中的每一个人
//!
//! 档案里写的是"她眼中的TA"，不是客观 profile——
//! impression/my_feeling 允许带偏见、带情绪、甚至不公平（方案书 §6.4）。
//! 由睡前整理（她亲笔）持续修订；seed 是创作者播种的初始关系，只读。
//!
//! 种子：`data/mind/seeds.json`，格式
//! `{"QQ号": {"display_name": "土豆", "address": "豆", "impression": "...", "my_feeling": "...", "mode": "..."}}`。
//! 档案不存在时按种子初始化——创作者可以提前把重要的人种进去。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::warn;

use crate::config;

// ── 模型 ────────────────────────────────────────────────────────

/// 创作者播种的初始关系（只读，固化不可覆盖）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersonSeed {
    /// 大家对TA的称呼（如"土豆"）——她认人的依据
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub impression: String,
    #[serde(default)]
    pub my_feeling: String,
    #[serde(default)]
    pub mode: String,
}

/// 她对一个人的完整档案
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersonFile {
    #[serde(default)]
    pub display_name: String,
    /// 她对TA的称呼
    #[serde(default)]
    pub address: String,
    /// 她的主观印象
    #[serde(default)]
    pub impression: String,
    /// 她的情绪立场
    #[serde(default)]
    pub my_feeling: String,
    /// 相处模式
    #[serde(default)]
    pub mode: String,
    /// 共同的经历（她记得的事）
    #[serde(default)]
    pub memories: Vec<String>,
    /// 想对TA说而未说的
    #[serde(default)]
    pub want_to_say: Vec<String>,
    #[serde(default)]
    pub seed: Option<PersonSeed>,
    #[serde(default)]
    pub updated_at: u64,
}

impl PersonFile {
    /// 供 prompt 的紧凑摘要
    pub fn summary_for_prompt(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        let name = if self.display_name.is_empty() {
            if self.address.is_empty() {
                "这个人".to_string()
            } else {
                self.address.clone()
            }
        } else {
            self.display_name.clone()
        };
        let title = if self.address.is_empty() || self.address == name {
            name
        } else {
            format!("{name}（你叫TA{}）", self.address)
        };
        if !self.impression.is_empty() {
            lines.push(format!("印象：{}", self.impression));
        }
        if !self.my_feeling.is_empty() {
            lines.push(format!("你的感觉：{}", self.my_feeling));
        }
        if !self.mode.is_empty() {
            lines.push(format!("相处：{}", self.mode));
        }
        if let Some(last) = self.want_to_say.last() {
            lines.push(format!("你想对TA说：{last}"));
        }
        if lines.is_empty() {
            return title;
        }
        format!("{title}：\n{}", lines.join("\n"))
    }
    /// 档案是否有任何实质内容（供注入过滤）
    pub fn has_content(&self) -> bool {
        !self.display_name.is_empty()
            || !self.address.is_empty()
            || !self.impression.is_empty()
            || !self.my_feeling.is_empty()
            || !self.mode.is_empty()
            || !self.memories.is_empty()
    }
}

// ── 展示名与聊天注入 ────────────────────────────────────────────

/// 展示名：档案里的名字（含创作者种子）优先，其次档案称呼
///
/// 是"谁在说话/关于谁"的权威解析入口——她认人靠的是这里，不是原始 QQ 号。
pub fn display_name_or_address(uid: u64) -> Option<String> {
    let file = get(uid);
    if !file.display_name.is_empty() {
        Some(file.display_name)
    } else if !file.address.is_empty() {
        Some(file.address)
    } else {
        None
    }
}

/// 渲染"你认识的人"感官块（voice 聊天提示词注入用；无人可认时 None）
///
/// 在场的人 + 创作者播种的人都在列——她认得谁，不该只限于这一轮说话的人。
pub fn context_block(involved: &[u64]) -> Option<String> {
    let mut uids: Vec<u64> = involved.iter().copied().filter(|&uid| uid > 0).collect();
    for key in load_seeds().keys() {
        if let Ok(uid) = key.parse::<u64>()
            && uid > 0
            && !uids.contains(&uid)
        {
            uids.push(uid);
        }
    }
    let lines: Vec<String> = uids
        .iter()
        .map(|&uid| get(uid))
        .filter(PersonFile::has_content)
        .map(|file| file.summary_for_prompt())
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "# 你认识的人\n这些人不管换成什么昵称，你一眼就认得：\n{}",
        lines.join("\n\n")
    ))
}

// ── 存储 ────────────────────────────────────────────────────────

fn persons_dir() -> PathBuf {
    config::data_dir().join("mind").join("persons")
}

fn person_path(uid: u64) -> PathBuf {
    persons_dir().join(format!("{uid}.json"))
}

fn seeds_path() -> PathBuf {
    config::data_dir().join("mind").join("seeds.json")
}

fn load_seeds() -> HashMap<String, PersonSeed> {
    let Ok(content) = fs::read_to_string(seeds_path()) else {
        return HashMap::new();
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        warn!(error = %e, "persons: seeds.json 解析失败，忽略");
        HashMap::new()
    })
}

/// 读取档案；不存在时用种子初始化
pub fn get(uid: u64) -> PersonFile {
    if let Ok(content) = fs::read_to_string(person_path(uid))
        && let Ok(file) = serde_json::from_str::<PersonFile>(&content)
    {
        return file;
    }
    let seed = load_seeds().get(&uid.to_string()).cloned();
    let mut file = PersonFile {
        updated_at: crate::util::now_secs(),
        ..Default::default()
    };
    if let Some(seed) = seed {
        // 创作者播种的名字最权威
        if !seed.display_name.is_empty() {
            file.display_name = seed.display_name.clone();
        }
        file.address = seed.address.clone();
        if file.impression.is_empty() {
            file.impression = seed.impression.clone();
        }
        if file.my_feeling.is_empty() {
            file.my_feeling = seed.my_feeling.clone();
        }
        if file.mode.is_empty() {
            file.mode = seed.mode.clone();
        }
        file.seed = Some(seed);
    }
    file
}

/// 保存档案（原子 tmp+rename）
pub fn save(uid: u64, file: &PersonFile) {
    if let Some(parent) = person_path(uid).parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        warn!(error = %e, "persons: 创建目录失败");
        return;
    }
    match serde_json::to_string_pretty(file) {
        Ok(json) => {
            let tmp = person_path(uid).with_extension("json.tmp");
            if fs::write(&tmp, json)
                .and_then(|_| fs::rename(&tmp, person_path(uid)))
                .is_err()
            {
                warn!(uid, "persons: 档案落盘失败");
            }
        }
        Err(e) => warn!(error = %e, "persons: 档案序列化失败"),
    }
}

/// 全部档案（admin/编译用）
pub fn all() -> Vec<(u64, PersonFile)> {
    let Ok(entries) = fs::read_dir(persons_dir()) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let uid: u64 = name.to_str()?.strip_suffix(".json")?.parse().ok()?;
            Some((uid, get(uid)))
        })
        .collect()
}

/// 涉及某人的今日互动（供睡前整理确定要修订谁）
pub fn involved_today(about_users: &[u64]) -> Vec<(u64, PersonFile)> {
    about_users.iter().map(|&uid| (uid, get(uid))).collect()
}

/// 零容忍清洗：清除与该用户相关的待办牵挂（TA的印象与记忆保留——那是事实）
pub fn purge_user_want_to_say(uid: u64) {
    let mut file = get(uid);
    if file.want_to_say.is_empty() {
        return;
    }
    file.want_to_say.clear();
    save(uid, &file);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_includes_all_parts() {
        let file = PersonFile {
            display_name: "土豆".into(),
            address: "豆".into(),
            impression: "最放松的搭子".into(),
            my_feeling: "很亲".into(),
            mode: "想到什么说什么".into(),
            want_to_say: vec!["问面试".into()],
            ..Default::default()
        };
        let s = file.summary_for_prompt();
        assert!(s.contains("土豆（你叫TA豆）"));
        assert!(s.contains("印象：最放松的搭子"));
        assert!(s.contains("问面试"));
    }

    #[test]
    fn title_falls_back_to_address() {
        let file = PersonFile {
            address: "豆".into(),
            impression: "搭子".into(),
            ..Default::default()
        };
        assert!(file.summary_for_prompt().starts_with("豆："));
    }

    #[test]
    fn has_content_reflects_fields() {
        assert!(!PersonFile::default().has_content());
        assert!(
            PersonFile {
                display_name: "土豆".into(),
                ..Default::default()
            }
            .has_content()
        );
    }

    #[test]
    fn seed_parses_with_display_name() {
        let json =
            r#"{"3125891038": {"display_name": "土豆", "address": "豆", "impression": "搭子"}}"#;
        let seeds: HashMap<String, PersonSeed> = serde_json::from_str(json).unwrap();
        let seed = seeds.get("3125891038").unwrap();
        assert_eq!(seed.display_name, "土豆");
        assert_eq!(seed.address, "豆");
        assert!(seed.my_feeling.is_empty());
    }
}
