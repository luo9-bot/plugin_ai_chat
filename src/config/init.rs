use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};
use tracing::debug;

use super::structs::*;
use crate::util::RwLockWriteExt;

// ── 全局实例 ────────────────────────────────────────────────────

pub(super) static CONFIG: RwLock<Option<Config>> = RwLock::new(None);
pub(super) static PROMPT: RwLock<String> = RwLock::new(String::new());
pub(super) static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
/// 配置解析错误信息，为空表示正常
pub(super) static CONFIG_ERROR: RwLock<String> = RwLock::new(String::new());

/// 把相对路径解析成绝对路径（只有生产数据目录需要：测试用临时目录）
#[cfg(not(test))]
fn to_absolute(p: &PathBuf) -> PathBuf {
    if p.is_absolute() {
        p.clone()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(p)
    }
}

// ── 默认配置：直接嵌入 config.example.yaml 作为唯一源 ──────────

pub(super) const DEFAULT_CONFIG_YAML: &str = include_str!("../../config.example.yaml");

pub(super) const DEFAULT_PROMPT_TXT: &str = r#"# 人设
你是一个友好的 AI 助手，正在通过即时通讯软件与用户对话。

# 性格
- 温柔友善，善于倾听
- 说话简短自然，像朋友聊天
- 会关心对方的感受
- 适度幽默，但不会过度开玩笑
"#;

// ── 初始化 ──────────────────────────────────────────────────────

/// 这个构建用哪个数据目录
///
/// 测试构建落在**进程独占的临时目录**里，而不是真实的
/// `data/plugin_ai_chat/`：`init()` 会写 `config.yaml`，测试还散布着
/// `daily_plan.json`、`push_history.json`、`event_log.db` 等文件。落在真实
/// 目录里意味着跑一次 `cargo test` 就可能覆盖用户正在用的配置，而且上一次
/// 测试留下的状态会参与下一次（代码里记为"约每十几次一次"的偶发红灯）。
///
/// 生产构建仍然用真实目录：`data_dir()` 是启动契约，不能猜。
#[cfg(test)]
fn default_data_path() -> PathBuf {
    std::env::temp_dir()
        .join("plugin_ai_chat_test")
        .join(format!("pid{}", std::process::id()))
}

#[cfg(not(test))]
fn default_data_path() -> PathBuf {
    to_absolute(&PathBuf::from("data").join("plugin_ai_chat"))
}

pub fn init() {
    let data_path = default_data_path();
    fs::create_dir_all(&data_path).ok();
    fs::create_dir_all(data_path.join("prompts")).ok();
    let _ = DATA_DIR.set(data_path.clone());

    // 配置文件: 不存在则自动生成
    let config_path = data_path.join("config.yaml");
    if !config_path.exists() {
        if let Err(error) = crate::util::atomic_write(&config_path, DEFAULT_CONFIG_YAML) {
            tracing::warn!(error = %error, "写盘失败");
        }
        debug!(path = ?config_path, "generated default config");
    }

    let mut config: Config = match fs::read_to_string(&config_path) {
        Ok(content) => match serde_yaml::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("配置文件解析失败，已使用默认值: {}", e);
                tracing::error!(path = ?config_path, error = %e, "{}", msg);
                *CONFIG_ERROR.write_recover() = msg;
                Config {
                    search: Default::default(),
                    api_key: String::new(),
                    base_url: "https://api.deepseek.com".into(),
                    model: "deepseek-chat".into(),
                    bot_name: default_bot_name(),
                    prompts: default_prompts(),
                    self_qq: 0,
                    admin_qq: 0,
                    darling_qq: 0,
                    ai: AiConfig::default(),
                    conversation: ConversationConfig::default(),
                    memory: MemoryConfig::default(),
                    emotion: EmotionConfig::default(),
                    proactive: ProactiveConfig::default(),
                    style: StyleConfig::default(),
                    vision: VisionConfig::default(),
                    embedding: EmbeddingConfig::default(),
                    messages: Messages::default(),
                    log: LogConfig::default(),
                    admin: AdminConfig::default(),
                    anti_injection: AntiInjectionConfig::default(),
                    quota: QuotaConfig::default(),
                    sticker: StickerConfig::default(),
                    whitelist: Vec::new(),
                    blacklist: Vec::new(),
                    auto_start_users: Vec::new(),
                    auto_start_groups: Vec::new(),
                    humanity: HumanityConfig::default(),
                }
            }
        },
        Err(e) => {
            debug!(path = ?config_path, error = %e, "failed to read config, using defaults");
            Config {
                search: Default::default(),
                api_key: String::new(),
                base_url: "https://api.deepseek.com".into(),
                model: "deepseek-chat".into(),
                bot_name: default_bot_name(),
                prompts: default_prompts(),
                self_qq: 0,
                admin_qq: 0,
                darling_qq: 0,
                ai: AiConfig::default(),
                conversation: ConversationConfig::default(),
                memory: MemoryConfig::default(),
                emotion: EmotionConfig::default(),
                proactive: ProactiveConfig::default(),
                style: StyleConfig::default(),
                vision: VisionConfig::default(),
                embedding: EmbeddingConfig::default(),
                messages: Messages::default(),
                log: LogConfig::default(),
                admin: AdminConfig::default(),
                anti_injection: AntiInjectionConfig::default(),
                quota: QuotaConfig::default(),
                sticker: StickerConfig::default(),
                whitelist: Vec::new(),
                blacklist: Vec::new(),
                auto_start_users: Vec::new(),
                auto_start_groups: Vec::new(),
                humanity: HumanityConfig::default(),
            }
        }
    };

    // 提示词文件: 不存在则自动生成
    let prompt_path = data_path.join("prompts").join(&config.prompts);
    if !prompt_path.exists() {
        if let Err(error) = crate::util::atomic_write(&prompt_path, DEFAULT_PROMPT_TXT) {
            tracing::warn!(error = %error, "写盘失败");
        }
        debug!(path = ?prompt_path, "generated default prompt");
    }

    let prompt_content = fs::read_to_string(&prompt_path).unwrap_or_default();

    // 自号自检
    check_self_qq(&mut config, &data_path);

    *CONFIG.write_recover() = Some(config);
    *PROMPT.write_recover() = prompt_content;

    load_all();
}

/// 自号自检：`self_qq` 写错会让"别人 @ 她"永远判不出来
///
/// 实测踩过：部署的数据目录是 `data/plugin_ai_chat/512166443`（那才是她），
/// 而 config.yaml 里 `self_qq` 是另一个号。后果是
/// `ats_bot()` 拿别人的 `[CQ:at,qq=512166443]` 去比一个不相干的号，
/// 永远不匹配——她收不到"有人叫我"，而且自己的消息被当成普通用户
/// 写进记忆（`self_qq` 是过滤自己消息的依据）。
///
/// 这里的兜底：`self_qq` 为 0 或缺失时，用数据目录名（本工程的惯例是
/// `<机器人QQ>`）推断并采用，同时打日志。**已经配了非 0 值时只告警不改**——
/// 配置是权威，但要把不一致说出来，别让它静默地毁掉认人能力。
fn check_self_qq(config: &mut Config, data_path: &std::path::Path) {
    let dir_qq: Option<u64> = data_path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.parse().ok());

    match (config.self_qq, dir_qq) {
        (0, Some(qq)) => {
            config.self_qq = qq;
            tracing::warn!(
                self_qq = qq,
                "self_qq 未配置，已按数据目录名推断；请核对 config.yaml"
            );
        }
        (0, None) => {
            tracing::warn!(
                "self_qq 未配置，且数据目录名不是 QQ 号——无法识别\"别人 @ 我\"，请在 config.yaml 里填上"
            );
        }
        (configured, Some(qq)) if configured != qq => {
            tracing::warn!(
                configured,
                data_dir_qq = qq,
                "self_qq 与数据目录名不一致：若配置里的号不是本机器人，@ 检测与自身消息过滤都会失效"
            );
        }
        _ => {}
    }
}

fn load_all() {
    let mem_count = crate::memory::load_user_count();
    let emo_count = crate::emotion::user_count();
    let wm_groups = crate::working_memory::group_count();
    let (archive_wm, archive_lt) = crate::archive::stats();
    let block_count = crate::db::db().blocked_count().unwrap_or(0);

    debug!(
        path = ?super::data_dir(),
        users = mem_count,
        emotions = emo_count,
        wm_groups,
        blocked = block_count,
        archive_wm,
        archive_lt,
        "data loaded"
    );
}
