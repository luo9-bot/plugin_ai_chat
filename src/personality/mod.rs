//! 人格系统：模板、特质、prompt 生成

mod store;
mod templates;

pub use store::*;
pub use templates::*;

/// 获取当前人格的 prompt 上下文
pub fn get_prompt_context() -> String {
    let store = store::PersonalityStore::load();
    let p = &store.current;
    let mut parts = Vec::new();

    if let Some((_, desc)) = templates::template_traits(&p.template) {
        parts.push(format!("# 人格模板\n{}", desc));
    }
    parts.push(templates::traits_to_prompt(&p.traits));
    if !p.custom_prompt.is_empty() {
        parts.push(p.custom_prompt.clone());
    }
    parts.join("\n\n")
}

/// 获取 verbosity 特质值
pub fn get_verbosity() -> f32 {
    store::PersonalityStore::load().current.traits.verbosity
}

/// 初始化时调用：返回当前人格名称
pub fn current_name() -> &'static str {
    "default"
}

/// 初始化时调用：返回快照数量
pub fn snapshot_count() -> usize {
    store::PersonalityStore::load().snapshots.len()
}

pub fn apply_template(template_name: &str) -> Result<String, String> {
    let Some((traits, _)) = templates::template_traits(template_name) else {
        return Err(format!("未知模板: {}", template_name));
    };
    let mut store = store::PersonalityStore::load();
    store.current.template = template_name.to_string();
    store.current.traits = traits;
    store.save();
    Ok(format!("已应用人格模板: {}", template_name))
}

pub fn adjust_trait(trait_name: &str, value: f32) -> Result<String, String> {
    let value = value.clamp(0.0, 1.0);
    let mut store = store::PersonalityStore::load();
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
    let mut store = store::PersonalityStore::load();
    store.snapshots.insert(name.to_string(), store.current.clone());
    store.save();
    Ok(format!("人格快照已保存: {}", name))
}

pub fn load_snapshot(name: &str) -> Result<String, String> {
    let mut store = store::PersonalityStore::load();
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
    store::PersonalityStore::load().snapshots.keys().cloned().collect()
}

pub fn set_custom_prompt(prompt: &str) {
    let mut store = store::PersonalityStore::load();
    store.current.custom_prompt = prompt.to_string();
    store.save();
}
