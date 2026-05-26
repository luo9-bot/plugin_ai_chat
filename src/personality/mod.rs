//! 人格系统（已弃用，保留空壳避免编译错误）

/// 返回空的人格上下文
pub fn get_prompt_context() -> String { String::new() }

/// 返回默认名称
pub fn current_name() -> String { "default".into() }

/// 返回 0
pub fn snapshot_count() -> usize { 0 }

/// 返回应用模板失败
pub fn apply_template(_name: &str) -> Result<String, String> { Err("人格系统已弃用".into()) }

/// 返回调整失败
pub fn adjust_trait(_trait: &str, _value: f32) -> Result<String, String> { Err("人格系统已弃用".into()) }

/// 返回保存失败
pub fn save_snapshot(_name: &str) -> Result<String, String> { Err("人格系统已弃用".into()) }

/// 返回加载失败
pub fn load_snapshot(_name: &str) -> Result<String, String> { Err("人格系统已弃用".into()) }

/// 返回空列表
pub fn list_snapshots() -> Vec<String> { vec![] }

/// 返回 verbosity 默认值
pub fn get_verbosity() -> f32 { 0.5 }
