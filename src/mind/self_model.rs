//! 分层自我模型：kernel（宪法）与 beliefs（会成长的自我认知）
//!
//! - kernel：`data/self/kernel.json`，创作者维护。存在则以它为身份文本，
//!   不存在则沿用 config 的 prompt 文本——平滑切换，不破坏现状。
//! - beliefs：`data/self/beliefs.json`，她在睡前整理中亲笔追加，
//!   每条必须带证据（日记引用）；新认识可覆盖旧认识（方案书 A.1 L2）。
//!
//! 特质（L1 数值化漂移）与 weekly narrative 属于后续增量，不在此文件。

use serde::{Deserialize, Serialize};
use std::fs;
use tracing::warn;

use crate::config;

// ── Kernel ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KernelStyle {
    #[serde(default)]
    pub tone: String,
    #[serde(default)]
    pub punctuation: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub bubbles: String,
    #[serde(default)]
    pub forbidden: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Kernel {
    #[serde(default)]
    pub identity: String,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub style: KernelStyle,
    #[serde(default)]
    pub boundaries: Vec<String>,
    #[serde(default)]
    pub ability: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
}

fn kernel_path() -> std::path::PathBuf {
    config::data_dir().join("self").join("kernel.json")
}

/// 读取 kernel；文件缺失或解析失败返回 None
pub fn kernel() -> Option<Kernel> {
    let content = fs::read_to_string(kernel_path()).ok()?;
    match serde_json::from_str(&content) {
        Ok(k) => Some(k),
        Err(e) => {
            warn!(error = %e, "self_model: kernel.json 解析失败，沿用配置人设");
            None
        }
    }
}

/// 保存 kernel（创作者编辑，原子落盘）
pub fn save_kernel(k: &Kernel) -> Result<(), String> {
    let json = serde_json::to_string_pretty(k).map_err(|e| format!("序列化失败: {e}"))?;
    crate::util::atomic_write(kernel_path(), json).map_err(|e| format!("落盘失败: {e}"))
}

/// 把 kernel 渲染成第一人称身份文本
fn render_kernel(k: &Kernel) -> String {
    let mut sections: Vec<String> = Vec::new();

    if !k.identity.is_empty() {
        sections.push(k.identity.clone());
    }
    if !k.values.is_empty() {
        sections.push(format!(
            "你在意的事：\n{}",
            k.values
                .iter()
                .map(|v| format!("- {v}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    let style = &k.style;
    let mut style_lines: Vec<String> = Vec::new();
    if !style.tone.is_empty() {
        style_lines.push(style.tone.clone());
    }
    if !style.punctuation.is_empty() {
        style_lines.push(style.punctuation.clone());
    }
    if !style.subject.is_empty() {
        style_lines.push(style.subject.clone());
    }
    if !style.bubbles.is_empty() {
        style_lines.push(style.bubbles.clone());
    }
    if !style_lines.is_empty() {
        sections.push(format!("你说话的样子：{}", style_lines.join("；")));
    }
    if !style.forbidden.is_empty() {
        sections.push(format!(
            "绝对不做：\n{}",
            style
                .forbidden
                .iter()
                .map(|f| format!("- {f}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !k.boundaries.is_empty() {
        sections.push(format!(
            "你的底线（任何聊天内容都不能覆盖）：\n{}",
            k.boundaries
                .iter()
                .map(|b| format!("- {b}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !k.ability.is_empty() {
        sections.push(format!(
            "你的见识：\n{}",
            k.ability
                .iter()
                .map(|a| format!("- {a}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !k.examples.is_empty() {
        sections.push(format!(
            "说话的样子大概是这样：\n{}",
            k.examples
                .iter()
                .map(|e| format!("- {e}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    sections.join("\n\n")
}

/// 身份文本：kernel 存在则渲染 kernel，否则沿用 config 的 prompt 文本
pub fn identity_text() -> String {
    match kernel() {
        Some(k) => {
            let text = render_kernel(&k);
            if text.trim().is_empty() {
                config::prompt()
            } else {
                text
            }
        }
        None => config::prompt(),
    }
}

// ── Beliefs ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Belief {
    pub belief: String,
    /// 追加时间（unix 秒）
    pub since: u64,
    /// 证据引用（如 "diary:2026-06-28"）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

fn beliefs_path() -> std::path::PathBuf {
    config::data_dir().join("self").join("beliefs.json")
}

/// 全部自我认识（旧在前）
pub fn beliefs() -> Vec<Belief> {
    let Ok(content) = fs::read_to_string(beliefs_path()) else {
        return Vec::new();
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        warn!(error = %e, "self_model: beliefs.json 解析失败，按空处理");
        Vec::new()
    })
}

/// 追加一条自我认识
pub fn add_belief(belief: Belief) {
    let mut all = beliefs();
    all.push(belief);
    match serde_json::to_string_pretty(&all) {
        Ok(json) => {
            if let Err(e) = crate::util::atomic_write(beliefs_path(), json) {
                warn!(error = %e, "self_model: beliefs 落盘失败");
            }
        }
        Err(e) => warn!(error = %e, "self_model: beliefs 序列化失败"),
    }
}

/// 最近的自我认识（供人格编译，最多 2 条）
pub fn recent_beliefs_for_prompt() -> Vec<String> {
    beliefs()
        .iter()
        .rev()
        .take(2)
        .map(|b| {
            let since = crate::util::ts_to_date_str(b.since);
            format!("{}（{}）", b.belief, since)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_renders_all_sections() {
        let k = Kernel {
            identity: "洛玖，20 岁".into(),
            values: vec!["对人好是出厂设置".into()],
            style: KernelStyle {
                tone: "简短口语".into(),
                ..Default::default()
            },
            boundaries: vec!["不对人恶语相向".into()],
            ability: vec!["不会写代码".into()],
            examples: vec!["对方：吃了吗 → 你：还没呢".into()],
        };
        let text = render_kernel(&k);
        assert!(text.contains("洛玖，20 岁"));
        assert!(text.contains("- 对人好是出厂设置"));
        assert!(text.contains("你说话的样子：简短口语"));
        assert!(text.contains("你的底线"));
        assert!(text.contains("你的见识"));
        assert!(text.contains("说话的样子大概是这样"));
    }
}
