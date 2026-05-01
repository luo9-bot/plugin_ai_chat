use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

// ── 顶层配置 ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_prompts")]
    pub prompts: String,
    #[serde(default)]
    pub self_qq: u64,
    #[serde(default)]
    pub admin_qq: u64,
    #[serde(default = "default_reply_style")]
    pub reply_style: String,

    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub conversation: ConversationConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub emotion: EmotionConfig,
    #[serde(default)]
    pub proactive: ProactiveConfig,
    #[serde(default)]
    pub messages: Messages,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AiConfig {
    #[serde(default = "default_frequency_penalty")]
    pub frequency_penalty: f64,
    #[serde(default = "default_presence_penalty")]
    pub presence_penalty: f64,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_top_p")]
    pub top_p: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_request_timeout")]
    pub request_timeout: u64,
    #[serde(default = "default_analysis_max_tokens")]
    pub analysis_max_tokens: u32,
    #[serde(default = "default_analysis_temperature")]
    pub analysis_temperature: f64,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            frequency_penalty: default_frequency_penalty(),
            presence_penalty: default_presence_penalty(),
            temperature: default_temperature(),
            top_p: default_top_p(),
            max_tokens: default_max_tokens(),
            request_timeout: default_request_timeout(),
            analysis_max_tokens: default_analysis_max_tokens(),
            analysis_temperature: default_analysis_temperature(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ConversationConfig {
    #[serde(default = "default_max_history")]
    pub max_history: usize,
    #[serde(default = "default_batch_timeout")]
    pub batch_timeout_ms: u64,
    #[serde(default = "default_typing_speed")]
    pub typing_speed: f64,
    #[serde(default = "default_max_typing_delay")]
    pub max_typing_delay_ms: u64,
    #[serde(default = "default_reply_follow_up_secs")]
    pub reply_follow_up_secs: u64,
    #[serde(default = "default_intrusiveness_weight")]
    pub intrusiveness_weight: f32,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            max_history: default_max_history(),
            batch_timeout_ms: default_batch_timeout(),
            typing_speed: default_typing_speed(),
            max_typing_delay_ms: default_max_typing_delay(),
            reply_follow_up_secs: default_reply_follow_up_secs(),
            intrusiveness_weight: default_intrusiveness_weight(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct MemoryConfig {
    #[serde(default = "default_normal_expire_days")]
    pub normal_expire_days: u64,
    #[serde(default = "default_important_fade_days")]
    pub important_fade_days: u64,
    #[serde(default = "default_auto_summarize_threshold")]
    pub auto_summarize_threshold: usize,
    #[serde(default = "default_working_memory_expire_hours")]
    pub working_memory_expire_hours: u64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            normal_expire_days: default_normal_expire_days(),
            important_fade_days: default_important_fade_days(),
            auto_summarize_threshold: default_auto_summarize_threshold(),
            working_memory_expire_hours: default_working_memory_expire_hours(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct EmotionConfig {
    #[serde(default = "default_decay_rate")]
    pub decay_rate: f32,
    #[serde(default = "default_decay_delay")]
    pub decay_delay_secs: u64,
    #[serde(default = "default_neutral_threshold")]
    pub neutral_threshold: f32,
    #[serde(default = "default_affinity_threshold")]
    pub affinity_threshold: f32,
}

impl Default for EmotionConfig {
    fn default() -> Self {
        Self {
            decay_rate: default_decay_rate(),
            decay_delay_secs: default_decay_delay(),
            neutral_threshold: default_neutral_threshold(),
            affinity_threshold: default_affinity_threshold(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProactiveConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_quiet_start")]
    pub quiet_start: u32,
    #[serde(default = "default_quiet_end")]
    pub quiet_end: u32,
    #[serde(default = "default_proactive_interval")]
    pub interval: u64,
    #[serde(default = "default_max_ignore")]
    pub max_ignore: u32,
    #[serde(default = "default_low_mood_multiplier")]
    pub low_mood_multiplier: f64,
    #[serde(default = "default_check_interval")]
    pub check_interval: u64,
}

impl Default for ProactiveConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            quiet_start: default_quiet_start(),
            quiet_end: default_quiet_end(),
            interval: default_proactive_interval(),
            max_ignore: default_max_ignore(),
            low_mood_multiplier: default_low_mood_multiplier(),
            check_interval: default_check_interval(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Messages {
    #[serde(default)]
    pub start: StartStopMsg,
    #[serde(default)]
    pub stop: StartStopMsg,
    #[serde(default)]
    pub forget: ForgetMsg,
    #[serde(default)]
    pub restart: StartStopMsg,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StartStopMsg {
    #[serde(default = "default_msg_ok")]
    pub success: String,
    #[serde(default = "default_msg_already")]
    pub redo: String,
}

impl Default for StartStopMsg {
    fn default() -> Self {
        Self { success: default_msg_ok(), redo: default_msg_already() }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ForgetMsg {
    #[serde(default = "default_forget_success")]
    pub success: String,
    #[serde(default = "default_forget_fail")]
    pub fail: String,
}

impl Default for ForgetMsg {
    fn default() -> Self {
        Self { success: default_forget_success(), fail: default_forget_fail() }
    }
}

// ── 默认值 ──────────────────────────────────────────────────────

fn default_prompts() -> String { "default.txt".into() }
fn default_frequency_penalty() -> f64 { 2.0 }
fn default_presence_penalty() -> f64 { 1.0 }
fn default_temperature() -> f64 { 1.3 }
fn default_top_p() -> f64 { 0.1 }
fn default_max_tokens() -> u32 { 4096 }
fn default_request_timeout() -> u64 { 60 }
fn default_analysis_max_tokens() -> u32 { 300 }
fn default_analysis_temperature() -> f64 { 0.3 }
fn default_max_history() -> usize { 10 }
fn default_batch_timeout() -> u64 { 6000 }
fn default_typing_speed() -> f64 { 5.0 }
fn default_max_typing_delay() -> u64 { 4000 }
fn default_reply_follow_up_secs() -> u64 { 300 }
fn default_intrusiveness_weight() -> f32 { 0.3 }
fn default_reply_style() -> String { "简短自然，一两句话即可，像朋友聊天".into() }
fn default_normal_expire_days() -> u64 { 30 }
fn default_important_fade_days() -> u64 { 7 }
fn default_auto_summarize_threshold() -> usize { 10 }
fn default_working_memory_expire_hours() -> u64 { 6 }
fn default_decay_rate() -> f32 { 0.15 }
fn default_decay_delay() -> u64 { 60 }
fn default_neutral_threshold() -> f32 { 0.15 }
fn default_affinity_threshold() -> f32 { 3.0 }
fn default_true() -> bool { true }
fn default_quiet_start() -> u32 { 23 }
fn default_quiet_end() -> u32 { 7 }
fn default_proactive_interval() -> u64 { 7200 }
fn default_max_ignore() -> u32 { 3 }
fn default_low_mood_multiplier() -> f64 { 2.0 }
fn default_check_interval() -> u64 { 60 }
fn default_msg_ok() -> String { "好的".into() }
fn default_msg_already() -> String { "已经开启啦".into() }
fn default_forget_success() -> String { "已遗忘对话记录".into() }
fn default_forget_fail() -> String { "没有找到对话记录".into() }

// ── 全局实例 ────────────────────────────────────────────────────

static CONFIG: OnceLock<Config> = OnceLock::new();
static PROMPT: OnceLock<String> = OnceLock::new();
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

fn to_absolute(p: &PathBuf) -> PathBuf {
    if p.is_absolute() {
        p.clone()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(p)
    }
}

// ── 自动生成默认配置 ────────────────────────────────────────────

const DEFAULT_CONFIG_YAML: &str = r#"# ============================================================
#  top.drluo.luo9-ai-chat 配置文件
#  首次运行自动生成，按需修改
# ============================================================

# ── API 配置 (必填) ─────────────────────────────────────────────
api_key: "your_api_key"
base_url: "https://api.deepseek.com/v1"
model: "deepseek-chat"
self_qq: 0                  # 机器人自身 QQ 号 (群消息定向判断必填)
admin_qq: 0                 # 管理员 QQ 号 (控制命令权限，0 = 所有人可用)
reply_style: "简短自然，一两句话即可，像朋友聊天"  # 回复风格/长度指导

# ── 提示词文件 (放在 prompts/ 目录下) ───────────────────────────
prompts: "default.txt"

# ── AI 调用参数 ─────────────────────────────────────────────────
ai:
  frequency_penalty: 2.0    # 频率惩罚 (-2.0~2.0)，越高越不重复
  presence_penalty: 1.0     # 存在惩罚 (-2.0~2.0)，越高越多样
  temperature: 1.3          # 温度 (0.0~2.0)，越高越随机
  top_p: 0.1                # 核采样 (0.0~1.0)，越低越集中
  max_tokens: 4096          # 最大回复 token
  request_timeout: 60       # 请求超时 (秒)
  analysis_max_tokens: 300  # 分析任务最大 token (记忆/情绪分析)
  analysis_temperature: 0.3 # 分析任务温度 (越低越确定)

# ── 对话参数 ─────────────────────────────────────────────────────
conversation:
  max_history: 10           # 对话历史保留轮数
  batch_timeout_ms: 6000    # 消息合并超时 (毫秒)
  typing_speed: 5.0         # 打字模拟速度 (字符/秒)
  max_typing_delay_ms: 4000 # 打字延迟上限 (毫秒)
  reply_follow_up_secs: 300 # 对话跟进超时 (秒)，超时后不再自动回复非@消息
  intrusiveness_weight: 0.3 # 主动插话权重 (0.0~1.0)，越低越容易回复无关消息

# ── 记忆参数 ─────────────────────────────────────────────────────
memory:
  normal_expire_days: 30     # 普通记忆过期天数
  important_fade_days: 7     # 重要记忆降权天数
  auto_summarize_threshold: 10 # 自动摘要触发阈值
  working_memory_expire_hours: 6 # 群聊工作记忆过期时间 (小时)

# ── 情绪参数 ─────────────────────────────────────────────────────
emotion:
  decay_rate: 0.15           # 情绪衰减速率 (每小时)
  decay_delay_secs: 60       # 衰减延迟 (秒)
  neutral_threshold: 0.15    # 平静恢复阈值
  affinity_threshold: 3.0    # 亲近感阈值 (次/小时)

# ── 主动对话参数 ─────────────────────────────────────────────────
proactive:
  enabled: true
  quiet_start: 23            # 免打扰开始 (24h)
  quiet_end: 7               # 免打扰结束
  interval: 7200             # 主动消息间隔 (秒)
  max_ignore: 3              # 忽略次数上限
  low_mood_multiplier: 2.0   # 低情绪间隔倍率
  check_interval: 60         # 周期检查间隔 (秒)

# ── 系统消息模板 ─────────────────────────────────────────────────
messages:
  start:
    success: "对话已开启，你可以开始聊天了。"
    redo: "对话已开启，无需重复操作。"
  stop:
    success: "对话已停止。"
    redo: "对话未开启，无需停止。"
  forget:
    success: "已遗忘对话记录。"
    fail: "没有找到对话记录。"
  restart:
    success: "对话已重启。"
    redo: "没有找到对话记录。"
"#;

const DEFAULT_PROMPT_TXT: &str = r#"# 人设
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
        println!("[ai_chat] generated default config at {:?}", config_path);
    }

    let config: Config = match fs::read_to_string(&config_path) {
        Ok(content) => serde_yaml::from_str(&content).expect("Failed to parse config.yaml"),
        Err(e) => {
            eprintln!("[ai_chat] failed to read {:?}: {}, using defaults", config_path, e);
            Config {
                api_key: String::new(),
                base_url: "https://api.deepseek.com/v1".into(),
                model: "deepseek-chat".into(),
                prompts: default_prompts(),
                self_qq: 0,
                admin_qq: 0,
                reply_style: default_reply_style(),
                ai: AiConfig::default(),
                conversation: ConversationConfig::default(),
                memory: MemoryConfig::default(),
                emotion: EmotionConfig::default(),
                proactive: ProactiveConfig::default(),
                messages: Messages::default(),
            }
        }
    };

    // 提示词文件: 不存在则自动生成
    let prompt_path = data_path.join("prompts").join(&config.prompts);
    if !prompt_path.exists() {
        fs::write(&prompt_path, DEFAULT_PROMPT_TXT).ok();
        println!("[ai_chat] generated default prompt at {:?}", prompt_path);
    }

    let prompt_content = match fs::read_to_string(&prompt_path) {
        Ok(content) => content,
        Err(_) => String::new(),
    };

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

    println!(
        "[ai_chat] data loaded from {:?}: {} users with memory, personality='{}' ({} snapshots), {} emotion states, {} proactive states, {} groups with working memory",
        data_dir(), mem_count, pers, snapshots, emo_count, proactive_count, wm_groups
    );
}

pub fn data_dir() -> &'static PathBuf {
    DATA_DIR.get().expect("Config not initialized")
}

pub fn get() -> &'static Config {
    CONFIG.get().expect("Config not initialized")
}

pub fn prompt() -> &'static str {
    PROMPT.get().map(|s| s.as_str()).unwrap_or("")
}
