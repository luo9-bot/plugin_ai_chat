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
        debug!(count = PROMPTS.get().unwrap().templates.len(), "prompt: manager initialized");
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

    /// 内置默认 prompt：如果文件不存在则写入
    fn ensure_defaults(dir: &std::path::Path, templates: &mut HashMap<String, String>) {
        let defaults: &[(&str, &str)] = &[
            ("core_rules", include_str!("../../defaults/core_rules.prompt")),
            ("luo9_chat", include_str!("../../defaults/luo9_chat.prompt")),
            ("luo9_timing_gate", include_str!("../../defaults/luo9_timing_gate.prompt")),
            ("timing_gate", include_str!("../../defaults/timing_gate.prompt")),
            ("planner", include_str!("../../defaults/planner.prompt")),
            ("replyer", include_str!("../../defaults/replyer.prompt")),
            ("post_analyze", include_str!("../../defaults/post_analyze.prompt")),
            ("review_conversation", include_str!("../../defaults/review_conversation.prompt")),
            ("emotion_analyze", include_str!("../../defaults/emotion_analyze.prompt")),
            ("crisis_mild", include_str!("../../defaults/crisis_mild.prompt")),
            ("crisis_severe", include_str!("../../defaults/crisis_severe.prompt")),
            ("crisis_ai_detect", include_str!("../../defaults/crisis_ai_detect.prompt")),
            ("memory_review", include_str!("../../defaults/memory_review.prompt")),
            ("memory_extract", include_str!("../../defaults/memory_extract.prompt")),
            ("memory_summarize", include_str!("../../defaults/memory_summarize.prompt")),
            ("self_reflect", include_str!("../../defaults/self_reflect.prompt")),
            ("proactive_message", include_str!("../../defaults/proactive_message.prompt")),
            ("mental_state_generate", include_str!("../../defaults/mental_state_generate.prompt")),
            ("vision_describe", include_str!("../../defaults/vision_describe.prompt")),
            ("daily_plan", include_str!("../../defaults/daily_plan.prompt")),
            ("learn_style", include_str!("../../defaults/learn_style.prompt")),
            ("sticker_content_filtration", include_str!("../../defaults/sticker_content_filtration.prompt")),
            ("sticker_select", include_str!("../../defaults/sticker_select.prompt")),
        ];

        for (name, content) in defaults {
            if !templates.contains_key(*name) {
                let path = dir.join(format!("{}.prompt", name));
                if !path.exists() {
                    std::fs::write(&path, content).ok();
                }
                templates.insert(name.to_string(), content.to_string());
            }
        }
    }
}
