//! Prompt 管理器：从 .prompt 文件加载模板，支持 {placeholder} 替换

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;
use tracing::{debug, warn};

/// 全局 PromptManager 单例
static PROMPTS: OnceLock<PromptManager> = OnceLock::new();

pub struct PromptManager {
    templates: HashMap<String, String>,
    data_dir: PathBuf,
}

impl PromptManager {
    /// 初始化：扫描 prompts/ 目录，加载所有 .prompt 和 .txt 文件
    pub fn init(data_dir: &Path) {
        let prompts_dir = data_dir.join("prompts");
        std::fs::create_dir_all(&prompts_dir).ok();

        let mut templates = HashMap::new();

        // 扫描目录中的 .prompt 和 .txt 文件
        if let Ok(entries) = std::fs::read_dir(&prompts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str());
                if ext == Some("prompt") || ext == Some("txt") {
                    let name = path
                        .file_stem()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        debug!(name = %name, bytes = content.len(), "prompt: loaded");
                        templates.insert(name, content);
                    }
                }
            }
        }

        // 内置默认 prompt：如果文件不存在则从编译时嵌入的内容生成
        Self::ensure_defaults(&prompts_dir, &mut templates);

        let _ = PROMPTS.set(PromptManager {
            templates,
            data_dir: data_dir.to_path_buf(),
        });
        debug!(
            count = PROMPTS.get().unwrap().templates.len(),
            "prompt: manager initialized"
        );
    }

    /// 获取全局实例（未初始化时 panic）
    pub fn get() -> &'static PromptManager {
        PROMPTS.get().expect("PromptManager not initialized")
    }

    /// 获取 prompt 模板并替换占位符
    ///
    /// 占位符格式：`{key}`，从 `vars` 映射中查找替换。
    /// 无匹配的占位符保持原样。
    pub fn render(&self, name: &str, vars: &HashMap<&str, &str>) -> String {
        let template = match self.templates.get(name) {
            Some(t) => t.as_str(),
            None => {
                warn!(name, "prompt: template not found");
                return String::new();
            }
        };
        let mut result = template.to_string();
        for (key, value) in vars {
            result = result.replace(&format!("{{{}}}", key), value);
        }
        result
    }

    /// 获取原始模板（不替换占位符）
    pub fn raw(&self, name: &str) -> &str {
        self.templates
            .get(name)
            .map(|s| s.as_str())
            .unwrap_or_else(|| {
                warn!(name, "prompt: template not found for raw()");
                ""
            })
    }

    /// 热重载指定 prompt（供 admin API 使用）
    pub fn reload(&mut self, name: &str) -> Result<(), String> {
        let dir = self.data_dir.join("prompts");
        // 尝试 .prompt 和 .txt 两种扩展名
        for ext in &["prompt", "txt"] {
            let path = dir.join(format!("{}.{}", name, ext));
            if path.exists() {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("read {}: {}", path.display(), e))?;
                self.templates.insert(name.to_string(), content);
                debug!(name, "prompt: reloaded");
                return Ok(());
            }
        }
        Err(format!("prompt file not found: {}", name))
    }

    /// 列出所有已加载的 prompt 名称
    pub fn list(&self) -> Vec<&str> {
        self.templates.keys().map(|s| s.as_str()).collect()
    }

    /// 内置默认 prompt：文件不存在时写入；存在时判断能否随版本升级刷新
    ///
    /// 这里有一个容易踩的坑：plugin 更新只换二进制，数据目录里的
    /// `*.prompt` 是上一版写下的旧文本。早先的实现只在文件**不存在**时
    /// 写入，于是"发言守门"这类在默认 prompt 上的加固对已部署实例
    /// 完全不生效——旧 prompt 会一直用下去。
    ///
    /// 现在分三种情况：
    /// - 文件不存在 → 写入内置默认
    /// - 文件内容与内置默认相同 → 无需处理
    /// - 文件内容与内置默认不同，但等于**历史上某个内置版本** → 判定为
    ///   "上一版自己写的、用户没动过"，安全升级到新版本
    /// - 其它情况 → 判定为用户自定义，保持原样
    fn ensure_defaults(dir: &std::path::Path, templates: &mut HashMap<String, String>) {
        for (name, current, previous) in defaults() {
            let path = dir.join(format!("{name}.prompt"));
            let on_disk = std::fs::read_to_string(&path).ok();

            match on_disk {
                None => {
                    std::fs::write(&path, current).ok();
                    templates.insert(name.to_string(), current.to_string());
                }
                Some(content) => {
                    let decision = decide_default(&content, current, &previous);
                    if decision.writes_file() {
                        std::fs::write(&path, current).ok();
                        if decision == DefaultDecision::Upgrade {
                            tracing::info!(name, "prompt: 内置模板已升级到当前版本");
                        }
                    }
                    let effective = match decision {
                        DefaultDecision::KeepUser => content,
                        _ => current.to_string(),
                    };
                    templates.insert(name.to_string(), effective);
                }
            }
        }
    }
}

/// 数据目录里已有的模板该怎么处理
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DefaultDecision {
    /// 与内置默认一致，无需处理
    Current,
    /// 是旧版内置文本、用户没改过 → 随版本升级
    Upgrade,
    /// 用户自定义 → 尊重，不覆盖
    KeepUser,
}

impl DefaultDecision {
    fn writes_file(self) -> bool {
        self == Self::Upgrade
    }
}

/// 决定磁盘上的模板内容是否需要被内置默认替换
///
/// 纯函数，便于对升级/保留两条路径分别做测试。
fn decide_default(on_disk: &str, current: &str, previous: &[&str]) -> DefaultDecision {
    if on_disk == current {
        DefaultDecision::Current
    } else if previous.contains(&on_disk) {
        DefaultDecision::Upgrade
    } else {
        DefaultDecision::KeepUser
    }
}

/// 内置模板清单：`(名字, 当前内容, 历史版本内容)`
///
/// 新增加固时，把被替换掉的旧文本追加进历史版本列表——这样从
/// 上一版升级上来的实例能自动迁移，而用户改过的模板不会被覆盖。
#[allow(clippy::type_complexity)]
fn defaults() -> Vec<(&'static str, &'static str, Vec<&'static str>)> {
    macro_rules! tpl {
        ($name:literal, $file:literal) => {
            ($name, include_str!($file), vec![])
        };
        ($name:literal, $file:literal, $($old:expr),+ $(,)?) => {
            ($name, include_str!($file), vec![$($old),+])
        };
    }
    vec![
        tpl!("core_rules", "../../defaults/core_rules.prompt"),
        tpl!(
            "voice",
            "../../defaults/voice.prompt",
            PREV_VOICE_BEFORE_SPEAK_GATE
        ),
        tpl!(
            "task_progress",
            "../../defaults/task_progress.prompt",
            PREV_TASK_PROGRESS_BEFORE_SELF_LOOP_FIX
        ),
        tpl!("post_analyze", "../../defaults/post_analyze.prompt"),
        tpl!("emotion_analyze", "../../defaults/emotion_analyze.prompt"),
        tpl!("crisis_ai_detect", "../../defaults/crisis_ai_detect.prompt"),
        tpl!("memory_review", "../../defaults/memory_review.prompt"),
        tpl!("memory_extract", "../../defaults/memory_extract.prompt"),
        tpl!("memory_summarize", "../../defaults/memory_summarize.prompt"),
        tpl!("vision_describe", "../../defaults/vision_describe.prompt"),
        tpl!("daily_plan", "../../defaults/daily_plan.prompt"),
        tpl!("weekly_plan", "../../defaults/weekly_plan.prompt"),
        tpl!("monthly_plan", "../../defaults/monthly_plan.prompt"),
        tpl!("learn_style", "../../defaults/learn_style.prompt"),
        tpl!(
            "sticker_content_filtration",
            "../../defaults/sticker_content_filtration.prompt"
        ),
        tpl!(
            "history_attention",
            "../../defaults/history_attention.prompt"
        ),
        tpl!(
            "reply_effect_judge",
            "../../defaults/reply_effect_judge.prompt"
        ),
    ]
}

/// 上一版内置 `voice.prompt`（"发言守门"加固之前）
///
/// 用于识别"用户没动过的旧内置模板"并自动升级。用户自定义过的
/// prompt 内容不会等于这段文本，因此不会被覆盖。
const PREV_VOICE_BEFORE_SPEAK_GATE: &str =
    include_str!("../../defaults/legacy/voice.pre-speak-gate.prompt");

/// 上一版内置 `task_progress.prompt`（自我强化环修复之前）
const PREV_TASK_PROGRESS_BEFORE_SELF_LOOP_FIX: &str =
    include_str!("../../defaults/legacy/task_progress.pre-self-loop-fix.prompt");

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENT: &str = "当前版本的内置模板";
    const LEGACY: &str = "上一版的内置模板";

    #[test]
    fn identical_content_needs_no_action() {
        assert_eq!(
            decide_default(CURRENT, CURRENT, &[LEGACY]),
            DefaultDecision::Current
        );
    }

    #[test]
    fn untouched_legacy_default_is_upgraded() {
        // 部署实例里是上一版写下的文件，用户没动过 → 应当升级，
        // 否则默认 prompt 上的加固永远到不了线上
        assert_eq!(
            decide_default(LEGACY, CURRENT, &[LEGACY]),
            DefaultDecision::Upgrade
        );
        assert!(DefaultDecision::Upgrade.writes_file());
    }

    #[test]
    fn user_customized_template_is_never_overwritten() {
        let mine = "我自己改过的模板，跟你内置的都不一样";
        assert_eq!(
            decide_default(mine, CURRENT, &[LEGACY]),
            DefaultDecision::KeepUser
        );
        assert!(!DefaultDecision::KeepUser.writes_file());
    }

    #[test]
    fn history_is_honoured_across_multiple_versions() {
        let older = "更早的一版";
        assert_eq!(
            decide_default(older, CURRENT, &[LEGACY, older]),
            DefaultDecision::Upgrade,
            "任意历史版本都应被识别为可升级"
        );
    }

    #[test]
    fn every_builtin_template_has_a_name_and_content() {
        for (name, current, previous) in defaults() {
            assert!(!name.is_empty());
            assert!(!current.is_empty(), "模板 {name} 内容为空");
            for old in previous {
                assert_ne!(old, current, "模板 {name} 的历史版本与当前版本相同");
            }
        }
    }

    #[test]
    fn legacy_snapshots_differ_from_current_versions() {
        // 历史快照的唯一用途是识别旧文件，与当前版本相同就没有意义
        assert_ne!(
            PREV_VOICE_BEFORE_SPEAK_GATE,
            include_str!("../../defaults/voice.prompt")
        );
        assert_ne!(
            PREV_TASK_PROGRESS_BEFORE_SELF_LOOP_FIX,
            include_str!("../../defaults/task_progress.prompt")
        );
    }
}
