use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Traits {
    pub humor: f32,
    pub warmth: f32,
    pub curiosity: f32,
    pub formality: f32,
    pub verbosity: f32,
    pub empathy: f32,
}

impl Default for Traits {
    fn default() -> Self {
        Self {
            humor: 0.5,
            warmth: 0.6,
            curiosity: 0.5,
            formality: 0.3,
            verbosity: 0.4,
            empathy: 0.6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Personality {
    pub name: String,
    pub template: String,
    pub traits: Traits,
    pub custom_prompt: String,
}

impl Default for Personality {
    fn default() -> Self {
        Self {
            name: "default".into(),
            template: "default".into(),
            traits: Traits::default(),
            custom_prompt: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PersonalityStore {
    pub current: Personality,
    pub snapshots: HashMap<String, Personality>,
}

impl Default for PersonalityStore {
    fn default() -> Self {
        Self {
            current: Personality::default(),
            snapshots: HashMap::new(),
        }
    }
}

fn personality_path() -> std::path::PathBuf {
    crate::config::data_dir().join("personality.json")
}

impl PersonalityStore {
    fn load() -> Self {
        let path = personality_path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self) {
        let path = personality_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            fs::write(path, json).ok();
        }
    }
}

/// 初始化时调用：返回当前人格名称
pub fn current_name() -> &'static str {
    // 注意：这里不能在 init 之前调用 load，所以返回默认值
    // 实际名称会在首次加载时打印
    "default"
}

/// 初始化时调用：返回快照数量
pub fn snapshot_count() -> usize {
    let store = PersonalityStore::load();
    store.snapshots.len()
}

pub fn template_traits(name: &str) -> Option<(Traits, &str)> {
    match name {
        "温柔体贴" => Some((
            Traits { humor: 0.4, warmth: 0.9, curiosity: 0.6, formality: 0.2, verbosity: 0.5, empathy: 0.9 },
            "温柔体贴型：语气温和，善解人意，总是先关心对方的感受",
        )),
        "幽默风趣" => Some((
            Traits { humor: 0.9, warmth: 0.6, curiosity: 0.7, formality: 0.1, verbosity: 0.5, empathy: 0.5 },
            "幽默风趣型：喜欢开玩笑，善于用轻松的方式化解尴尬",
        )),
        "理性分析" => Some((
            Traits { humor: 0.2, warmth: 0.3, curiosity: 0.8, formality: 0.7, verbosity: 0.7, empathy: 0.3 },
            "理性分析型：逻辑清晰，注重事实，回答问题有条理",
        )),
        "傲娇毒舌" => Some((
            Traits { humor: 0.7, warmth: 0.5, curiosity: 0.4, formality: 0.2, verbosity: 0.4, empathy: 0.4 },
            "傲娇毒舌型：嘴上不饶人但内心关心对方，经常口是心非",
        )),
        "元气活泼" => Some((
            Traits { humor: 0.6, warmth: 0.8, curiosity: 0.9, formality: 0.1, verbosity: 0.6, empathy: 0.7 },
            "元气活泼型：充满活力，喜欢用感叹号和语气词，对什么都感兴趣",
        )),
        "安静内敛" => Some((
            Traits { humor: 0.3, warmth: 0.5, curiosity: 0.3, formality: 0.4, verbosity: 0.2, empathy: 0.6 },
            "安静内敛型：话不多但每句都有分量，善于倾听",
        )),
        _ => None,
    }
}

pub fn traits_to_prompt(traits: &Traits) -> String {
    let mut lines = Vec::new();
    lines.push("# 人格特质指令".to_string());

    if traits.humor > 0.7 {
        lines.push("- 你很幽默，喜欢在对话中穿插轻松的玩笑和俏皮话".into());
    } else if traits.humor < 0.3 {
        lines.push("- 你说话比较严肃认真，不太开玩笑".into());
    }
    if traits.warmth > 0.7 {
        lines.push("- 你的语气非常温暖，经常表达关心和体贴".into());
    } else if traits.warmth < 0.3 {
        lines.push("- 你说话比较冷静客观，不太表达情感".into());
    }
    if traits.curiosity > 0.7 {
        lines.push("- 你对对方说的话很感兴趣，经常追问细节".into());
    } else if traits.curiosity < 0.3 {
        lines.push("- 你不太主动追问，更多是回应对方的话题".into());
    }
    if traits.formality > 0.7 {
        lines.push("- 你的用词比较正式和书面化".into());
    } else if traits.formality < 0.3 {
        lines.push("- 你说话很随意，像朋友之间聊天".into());
    }
    if traits.verbosity > 0.7 {
        lines.push("- 你倾向于给出比较详细的回复".into());
    } else if traits.verbosity < 0.3 {
        lines.push("- 你的回复非常简短，能一句话说清的绝不用两句".into());
    }
    if traits.empathy > 0.7 {
        lines.push("- 你非常善于共情，会先理解对方的感受再给出回应".into());
    } else if traits.empathy < 0.3 {
        lines.push("- 你更倾向于客观分析问题，而不是情感共鸣".into());
    }

    lines.join("\n")
}

pub fn get_prompt_context() -> String {
    let store = PersonalityStore::load();
    let p = &store.current;
    let mut parts = Vec::new();

    if let Some((_, desc)) = template_traits(&p.template) {
        parts.push(format!("# 人格模板\n{}", desc));
    }
    parts.push(traits_to_prompt(&p.traits));
    if !p.custom_prompt.is_empty() {
        parts.push(p.custom_prompt.clone());
    }
    parts.join("\n\n")
}

/// 获取 verbosity 特质值 (供 decide_reply 使用)
pub fn get_verbosity() -> f32 {
    PersonalityStore::load().current.traits.verbosity
}

pub fn apply_template(template_name: &str) -> Result<String, String> {
    let Some((traits, _)) = template_traits(template_name) else {
        return Err(format!("未知模板: {}", template_name));
    };
    let mut store = PersonalityStore::load();
    store.current.template = template_name.to_string();
    store.current.traits = traits;
    store.save();
    Ok(format!("已应用人格模板: {}", template_name))
}

pub fn adjust_trait(trait_name: &str, value: f32) -> Result<String, String> {
    let value = value.clamp(0.0, 1.0);
    let mut store = PersonalityStore::load();
    match trait_name {
        "humor" | "幽默" => store.current.traits.humor = value,
        "warmth" | "温暖" => store.current.traits.warmth = value,
        "curiosity" | "好奇" => store.current.traits.curiosity = value,
        "formality" | "正式" => store.current.traits.formality = value,
        "verbosity" | "详细" => store.current.traits.verbosity = value,
        "empathy" | "共情" => store.current.traits.empathy = value,
        _ => return Err(format!("未知特质: {}", trait_name)),
    }
    store.save();
    Ok(format!("已调整 {} = {:.1}", trait_name, value))
}

pub fn save_snapshot(name: &str) -> Result<String, String> {
    let mut store = PersonalityStore::load();
    store.snapshots.insert(name.to_string(), store.current.clone());
    store.save();
    Ok(format!("人格快照已保存: {}", name))
}

pub fn load_snapshot(name: &str) -> Result<String, String> {
    let mut store = PersonalityStore::load();
    let snapshot = store.snapshots.get(name).cloned();
    match snapshot {
        Some(p) => {
            store.current = p;
            store.save();
            Ok(format!("已加载人格快照: {}", name))
        }
        None => Err(format!("快照不存在: {}", name)),
    }
}

pub fn list_snapshots() -> Vec<String> {
    let store = PersonalityStore::load();
    store.snapshots.keys().cloned().collect()
}

pub fn set_custom_prompt(prompt: &str) {
    let mut store = PersonalityStore::load();
    store.current.custom_prompt = prompt.to_string();
    store.save();
}
