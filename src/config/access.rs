use std::fs;
use std::path::PathBuf;
use tracing::debug;

use super::init::{CONFIG, CONFIG_ERROR, DATA_DIR, PROMPT};
use super::structs::Config;
use crate::util::{RwLockReadExt, RwLockWriteExt};

/// 数据目录
///
/// 这是**启动契约**：把状态写到"猜出来的目录"比在这里失败更糟（数据会散到
/// 错误位置且无人察觉）。因此本函数是"配置必须先初始化"的断言，也是全仓
/// 少数被显式豁免 `expect_used` 的位置之一。
#[allow(clippy::expect_used)]
pub fn data_dir() -> &'static PathBuf {
    DATA_DIR.get().expect("Config not initialized")
}

/// 数据目录；配置尚未初始化时返回 `None`
///
/// 给"可能在启动流程之前被调用"的模块用（状态库的惰性打开）。
/// 这类模块不该因为启动顺序而 panic——那是把顺序错误变成崩溃。
pub fn try_data_dir() -> Option<&'static PathBuf> {
    DATA_DIR.get()
}

/// 获取配置的克隆（每次调用会 clone，但 Config 很小且调用不频繁）
///
/// 同 [`data_dir`]：未初始化即访问配置是启动顺序错误，必须立刻可见。
#[allow(clippy::expect_used)]
pub fn get() -> Config {
    CONFIG
        .read_recover()
        .as_ref()
        .expect("Config not initialized")
        .clone()
}

/// 获取配置解析错误信息，为空表示正常
pub fn error_message() -> String {
    CONFIG_ERROR.read_recover().clone()
}

/// 重新载入配置文件（热重载，无需重启插件）
pub fn reload() -> Result<(), String> {
    let config_path = data_dir().join("config.yaml");
    let content = fs::read_to_string(&config_path).map_err(|e| format!("读取配置失败: {}", e))?;
    let config: Config = match serde_yaml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("配置文件解析失败: {}", e);
            *CONFIG_ERROR.write_recover() = msg.clone();
            return Err(msg);
        }
    };

    // 解析成功，清除错误标记
    *CONFIG_ERROR.write_recover() = String::new();

    // 更新提示词
    let prompt_path = data_dir().join("prompts").join(&config.prompts);
    if prompt_path.exists() {
        let prompt_content = fs::read_to_string(&prompt_path).unwrap_or_default();
        *PROMPT.write_recover() = prompt_content;
    }

    *CONFIG.write_recover() = Some(config);
    debug!("config: hot-reloaded successfully");
    Ok(())
}

pub fn prompt() -> String {
    PROMPT.read_recover().clone()
}

/// 保存配置：**类型化序列化 + 原子落盘**
///
/// 这是唯一的保存路径。它取代了原先 307 行的"逐行文本重写器"
/// （`save_config_with_comments`）——那个实现靠缩进栈猜层级，
/// 在嵌套字段、非两空格缩进、含 `#` 的字符串值上会**静默产出错误 YAML**
/// 并且无条件返回 `Ok`（见 `.docs/counter-world-design.md` §3.2）。
///
/// 注释不承载语义，因此不再保留：想留笔记请写进 `config.notes.md`。
/// 代价是明确的、可预期的；而"偶尔丢一个嵌套字段"不是。
///
/// 写盘前先反序列化校验一次：序列化/反序列化不对称（`skip_serializing`、
/// 自定义 `deserialize_with` 等）会让配置变成"写得进、读不出"，
/// 那正是"改坏配置导致插件起不来"的成因。
pub fn save(config: &Config) -> Result<(), String> {
    let path = data_dir().join("config.yaml");
    let yaml = serde_yaml::to_string(config).map_err(|e| format!("序列化配置失败: {e}"))?;

    // 校验：写出去的内容必须能被自己读回来
    if let Err(e) = serde_yaml::from_str::<Config>(&yaml) {
        return Err(format!("配置无法通过回读校验，已放弃写入: {e}"));
    }

    crate::util::atomic_write(&path, yaml).map_err(|e| format!("写入配置失败: {e}"))?;
    debug!("config: saved");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::init::DEFAULT_CONFIG_YAML;
    use super::*;

    /// 默认配置的参考实例：默认值的唯一真源就是模板本身
    fn reference_config() -> Config {
        serde_yaml::from_str(DEFAULT_CONFIG_YAML).expect("模板必须是合法的 Config")
    }

    /// 模板（`config.example.yaml`）必须始终能解析成 `Config`。
    ///
    /// 模板是 `include_str!` 进二进制的默认值来源：一旦它与结构体漂移，
    /// 新装用户的配置就会带着解析错误启动。
    #[test]
    fn example_template_parses_into_config() {
        let parsed: Result<Config, _> = serde_yaml::from_str(DEFAULT_CONFIG_YAML);
        assert!(
            parsed.is_ok(),
            "config.example.yaml 无法解析为 Config：{:?}",
            parsed.err()
        );
    }

    /// 保存路径不得丢值：改两个**嵌套**字段，序列化后重新解析必须还在。
    ///
    /// `CLAUDE.md` 明确记录过这个失效模式——模板里被注释掉的嵌套字段
    /// 既进不了 `handled_keys` 也进不了缩进栈，保存时会**静默丢失**。
    /// 类型化保存从结构上消除了这个失败模式，这条测试把它钉住。
    #[test]
    fn nested_values_survive_serialize_and_reparse() {
        let mut config = reference_config();

        config.conversation.batch_timeout_ms = 4321;
        config.style.max_reply_chars = 77;

        let yaml = serde_yaml::to_string(&config).expect("序列化");
        let reloaded: Config = serde_yaml::from_str(&yaml).expect("回读");

        assert_eq!(
            reloaded.conversation.batch_timeout_ms, 4321,
            "嵌套字段在序列化时丢失"
        );
        assert_eq!(reloaded.style.max_reply_chars, 77, "嵌套字段在序列化时丢失");
    }

    /// 配置往返必须是恒等。
    ///
    /// "能写出去但读回来不一样"（`skip_serializing`、自定义
    /// `deserialize_with` 之类的不对称）会在这里暴露，而不是在用户升级后
    /// 发现配置被悄悄重置。
    #[test]
    fn config_round_trip_is_identity() {
        let original = reference_config();
        let yaml = serde_yaml::to_string(&original).expect("序列化");
        let reloaded: Config = serde_yaml::from_str(&yaml).expect("回读");

        let yaml_again = serde_yaml::to_string(&reloaded).expect("再次序列化");
        assert_eq!(yaml, yaml_again, "配置往返不是恒等");
    }

    /// **三处同步规则的第三条**：`ConfigView.vue` 的 `sections` 里每个
    /// 字段路径都必须真的存在于配置结构体中。
    ///
    /// 否则网页上会出现一个"改了没用"的旋钮——`CLAUDE.md` 把这条列为
    /// 靠人眼维持的一致性之一。这里把它变成编译期就能跑到的断言：
    /// 结构体删字段而忘了改前端，测试立刻失败。
    #[test]
    fn admin_ui_field_paths_resolve_in_config() {
        let config_value: serde_yaml::Value =
            serde_yaml::to_value(reference_config()).expect("配置必须可序列化");
        let view = include_str!("../../frontend/src/views/ConfigView.vue");

        let pattern = regex::Regex::new(r"key:\s*'([A-Za-z0-9_.]+)'").expect("正则必须合法");
        let unresolved: Vec<&str> = pattern
            .captures_iter(view)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str()))
            .filter(|path| resolve_config_path(&config_value, path).is_none())
            .collect();

        assert!(
            unresolved.is_empty(),
            "ConfigView.vue 指向了不存在的配置路径（网页旋钮会失效）：{unresolved:?}"
        );
    }

    /// 按点分路径在 YAML 映射里查找；返回 `None` 表示路径不存在
    fn resolve_config_path<'a>(
        value: &'a serde_yaml::Value,
        path: &str,
    ) -> Option<&'a serde_yaml::Value> {
        path.split('.')
            .try_fold(value, |current, segment| current.get(segment))
    }

    /// 模板里不能有**结构体不认识**的键。
    ///
    /// `Config` 没有 `deny_unknown_fields`，所以模板里残留的键会被静默忽略：
    /// 它既不报错也不生效，只是让文档承诺一个不存在的旋钮。
    /// （清理前模板里就有 10 个这样的死键。）
    ///
    /// 反方向——结构体有而模板没有——不会解析失败（会拿到代码内建默认值），
    /// 因此这里只钉住会造成"文档撒谎"的那一侧。
    #[test]
    fn template_has_no_unknown_keys() {
        let template: serde_yaml::Value =
            serde_yaml::from_str(DEFAULT_CONFIG_YAML).expect("模板必须是合法 YAML");
        let known: serde_yaml::Value =
            serde_yaml::to_value(reference_config()).expect("配置必须可序列化");

        let mut template_paths = Vec::new();
        collect_leaf_paths(&template, String::new(), &mut template_paths);

        let unknown: Vec<String> = template_paths
            .into_iter()
            .filter(|path| resolve_config_path(&known, path).is_none())
            .collect();

        assert!(
            unknown.is_empty(),
            "config.example.yaml 含有结构体不认识的键（会被静默忽略）：{unknown:?}"
        );
    }

    /// 收集叶子字段的点分路径（非空映射继续下钻，其余算叶子）
    fn collect_leaf_paths(value: &serde_yaml::Value, prefix: String, paths: &mut Vec<String>) {
        match value {
            serde_yaml::Value::Mapping(map) if !map.is_empty() => {
                for (key, child) in map {
                    let Some(name) = key.as_str() else { continue };
                    let path = if prefix.is_empty() {
                        name.to_string()
                    } else {
                        format!("{prefix}.{name}")
                    };
                    collect_leaf_paths(child, path, paths);
                }
            }
            _ => {
                if !prefix.is_empty() {
                    paths.push(prefix);
                }
            }
        }
    }
}
