use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use tracing::debug;

use super::structs::*;

// ── 全局实例 ────────────────────────────────────────────────────

pub(super) static CONFIG: OnceLock<Config> = OnceLock::new();
pub(super) static PROMPT: OnceLock<String> = OnceLock::new();
pub(super) static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

fn to_absolute(p: &PathBuf) -> PathBuf {
    if p.is_absolute() {
        p.clone()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(p)
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

pub fn init() {
    let data_path = to_absolute(&PathBuf::from("data").join("plugin_ai_chat"));
    fs::create_dir_all(&data_path).ok();
    fs::create_dir_all(data_path.join("prompts")).ok();
    let _ = DATA_DIR.set(data_path.clone());

    // 配置文件: 不存在则自动生成
    let config_path = data_path.join("config.yaml");
    if !config_path.exists() {
        fs::write(&config_path, DEFAULT_CONFIG_YAML).ok();
        debug!(path = ?config_path, "generated default config");
    }

    let config: Config = match fs::read_to_string(&config_path) {
        Ok(content) => serde_yaml::from_str(&content).expect("Failed to parse config.yaml"),
        Err(e) => {
            debug!(path = ?config_path, error = %e, "failed to read config, using defaults");
            Config {
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
                self_reflection: SelfReflectionConfig::default(),
                mental_state: MentalStateConfig::default(),
                style: StyleConfig::default(),
                vision: VisionConfig::default(),
                embedding: EmbeddingConfig::default(),
                messages: Messages::default(),
                log: LogConfig::default(),
                sync: SyncConfig::default(),
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
        fs::write(&prompt_path, DEFAULT_PROMPT_TXT).ok();
        debug!(path = ?prompt_path, "generated default prompt");
    }

    let prompt_content = fs::read_to_string(&prompt_path).unwrap_or_default();

    let _ = CONFIG.set(config);
    let _ = PROMPT.set(prompt_content);

    load_all();
}

fn load_all() {
    let mem_count = crate::memory::load_user_count();
    let pers = crate::personality::current_name();
    let snapshots = crate::personality::snapshot_count();
    let emo_count = crate::emotion::user_count();
    let proactive_count = crate::proactive::user_count();
    let wm_groups = crate::working_memory::group_count();
    let self_thoughts = crate::self_memory::load_count();
    let mental_count = crate::mental_state::load_count();
    let (archive_wm, archive_lt) = crate::archive::stats();
    let block_count = crate::blocklist::load_count();

    debug!(
        path = ?super::data_dir(),
        users = mem_count,
        personality = %pers,
        snapshots,
        emotions = emo_count,
        proactive = proactive_count,
        wm_groups,
        self_thoughts,
        mental_state = mental_count,
        blocked = block_count,
        archive_wm,
        archive_lt,
        "data loaded"
    );
}
