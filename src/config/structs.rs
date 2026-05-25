use serde::{Deserialize, Serialize};

// ── 顶层配置 ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_bot_name")]
    pub bot_name: String,
    #[serde(default = "default_prompts")]
    pub prompts: String,
    #[serde(default)]
    pub self_qq: u64,
    #[serde(default)]
    pub admin_qq: u64,
    /// 认定的人的 QQ 号（对这个人会有特殊的情感反应）
    #[serde(default)]
    pub darling_qq: u64,
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
    pub self_reflection: SelfReflectionConfig,
    #[serde(default)]
    pub mental_state: MentalStateConfig,
    #[serde(default)]
    pub style: StyleConfig,
    #[serde(default)]
    pub vision: VisionConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub messages: Messages,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub admin: AdminConfig,
    #[serde(default)]
    pub anti_injection: AntiInjectionConfig,
    #[serde(default)]
    pub quota: QuotaConfig,
    #[serde(default)]
    pub sticker: StickerConfig,
    /// 白名单：只允许这些用户使用私聊（为空则不限制）
    #[serde(default)]
    pub whitelist: Vec<u64>,
    /// 黑名单：禁止这些用户使用私聊（为空则不限制）
    #[serde(default)]
    pub blacklist: Vec<u64>,
    /// 默认开启私聊的用户列表（无需手动发送"开启对话"）
    #[serde(default)]
    pub auto_start_users: Vec<u64>,
    /// 默认开启群聊的群号列表（无需管理员手动发送"开启对话"）
    #[serde(default)]
    pub auto_start_groups: Vec<u64>,
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
    /// 是否允许括号内的动作/表情描述，如"（笑了笑）"，默认 true
    #[serde(default = "default_action_descriptions")]
    pub action_descriptions: bool,
    /// 对同一用户的回复冷却时间 (秒)，防止连续回复刷屏，默认 15
    #[serde(default = "default_reply_cooldown_secs")]
    pub reply_cooldown_secs: u64,
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
            action_descriptions: default_action_descriptions(),
            reply_cooldown_secs: default_reply_cooldown_secs(),
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

#[derive(Debug, Deserialize, Clone)]
pub struct SelfReflectionConfig {
    /// 自我反思间隔 (秒)，默认 1800 (30分钟)
    #[serde(default = "default_reflection_interval")]
    pub interval: u64,
    /// 注入 prompt 的自我记忆条数上限，默认 8
    /// 所有自我记忆都会永久保存，此值只控制每次对话注入多少条最近的想法
    #[serde(default = "default_max_thoughts")]
    pub max_thoughts: usize,
    /// 对话结束后多久触发反思 (秒)，默认 120 (2分钟)
    #[serde(default = "default_post_conversation_delay")]
    pub post_conversation_delay_secs: u64,
}

impl Default for SelfReflectionConfig {
    fn default() -> Self {
        Self {
            interval: default_reflection_interval(),
            max_thoughts: default_max_thoughts(),
            post_conversation_delay_secs: default_post_conversation_delay(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct MentalStateConfig {
    #[serde(default = "default_concerns_max")]
    pub concerns_max: usize,
    #[serde(default = "default_concern_decay_rate")]
    pub concern_decay_rate: f32,
    #[serde(default = "default_deliberations_max")]
    pub deliberations_max: usize,
    #[serde(default = "default_deliberation_decay_rate")]
    pub deliberation_decay_rate: f32,
    #[serde(default = "default_defect_base_probability")]
    pub defect_base_probability: f32,
}

impl Default for MentalStateConfig {
    fn default() -> Self {
        Self {
            concerns_max: default_concerns_max(),
            concern_decay_rate: default_concern_decay_rate(),
            deliberations_max: default_deliberations_max(),
            deliberation_decay_rate: default_deliberation_decay_rate(),
            defect_base_probability: default_defect_base_probability(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct VisionConfig {
    /// 识图 API key，为空则禁用识图功能
    #[serde(default)]
    pub api_key: String,
    /// API base URL
    #[serde(default = "default_vision_base_url")]
    pub base_url: String,
    /// 识图模型
    #[serde(default = "default_vision_model")]
    pub model: String,
    /// 最大回复 token 数
    #[serde(default = "default_vision_max_tokens")]
    pub max_tokens: u32,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: default_vision_base_url(),
            model: default_vision_model(),
            max_tokens: default_vision_max_tokens(),
        }
    }
}

impl VisionConfig {
    pub fn enabled(&self) -> bool {
        !self.api_key.is_empty()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct EmbeddingConfig {
    /// Embedding API key，为空则禁用 embedding
    #[serde(default)]
    pub api_key: String,
    /// API base URL
    #[serde(default = "default_embedding_base_url")]
    pub base_url: String,
    /// Embedding 模型
    #[serde(default = "default_embedding_model")]
    pub model: String,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: default_embedding_base_url(),
            model: default_embedding_model(),
        }
    }
}

impl EmbeddingConfig {
    pub fn enabled(&self) -> bool {
        !self.api_key.is_empty()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct StyleConfig {
    /// 单条回复最大字数，默认 30
    #[serde(default = "default_max_reply_chars")]
    pub max_reply_chars: usize,
    /// 是否省略主语 ("我觉得无聊" → "无聊")，默认 true
    #[serde(default = "default_true")]
    pub omit_subject: bool,
    /// 标点风格: "casual"(不加句号，用换行分隔) | "formal"(正常标点)，默认 "casual"
    #[serde(default = "default_punctuation_style")]
    pub punctuation_style: String,
    /// 默认回复风格描述
    #[serde(default)]
    pub reply_style: String,
    /// 备选回复风格列表（按概率随机选择）
    #[serde(default)]
    pub multiple_reply_styles: Vec<String>,
    /// 使用备选风格的概率 (0.0-1.0)，默认 0.3
    #[serde(default = "default_style_random_probability")]
    pub style_random_probability: f64,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            max_reply_chars: default_max_reply_chars(),
            omit_subject: true,
            punctuation_style: default_punctuation_style(),
            reply_style: String::new(),
            multiple_reply_styles: Vec::new(),
            style_random_probability: default_style_random_probability(),
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

#[derive(Debug, Deserialize, Clone)]
pub struct LogConfig {
    /// 是否启用日志文件输出，默认 true
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 日志级别: "debug" | "info" | "warn" | "error"，默认 "info"
    #[serde(default = "default_log_level")]
    pub level: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            level: default_log_level(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SyncConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_url: String,
    #[serde(default = "default_db_name")]
    pub db_name: String,
    #[serde(default)]
    pub mongodb_uri: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub expose: bool,
}

fn default_db_name() -> String { "memory_default".into() }

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_url: String::new(),
            db_name: default_db_name(),
            mongodb_uri: String::new(),
            display_name: String::new(),
            icon: String::new(),
            expose: false,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct AdminConfig {
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_admin_port")]
    pub port: u16,
}

fn default_admin_port() -> u16 { 17000 }

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            port: 17000,
        }
    }
}

// ── 防注入配置 ────────────────────────────────────────────────────
// 注意：防注入系统始终开启，不可关闭

#[derive(Debug, Deserialize, Clone)]
#[derive(Default)]
pub struct AntiInjectionConfig {
    /// 输入层配置
    #[serde(default)]
    pub input: InputFilterConfig,
    /// 输出层配置
    #[serde(default)]
    pub output: OutputFilterConfig,
    /// 行为层配置
    #[serde(default)]
    pub behavior: BehaviorConfig,
}


#[derive(Debug, Deserialize, Clone)]
pub struct InputFilterConfig {
    /// 最大消息长度 (超过则截断)
    #[serde(default = "default_max_message_length")]
    pub max_message_length: usize,
    /// 敏感内容处理方式: "replace" | "block"
    /// 最低等级为 "replace"，可配置为 "block"
    #[serde(default = "default_sensitive_action")]
    pub sensitive_action: String,
}

impl Default for InputFilterConfig {
    fn default() -> Self {
        Self {
            max_message_length: 2000,
            sensitive_action: "replace".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct OutputFilterConfig {
    /// 检测到问题时的处理: "replace" | "block"
    /// 最低等级为 "replace"，可配置为 "block"
    #[serde(default = "default_output_action")]
    pub action: String,
}

impl Default for OutputFilterConfig {
    fn default() -> Self {
        Self {
            action: "replace".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct BehaviorConfig {
    /// 是否启用频率限制
    #[serde(default = "default_true")]
    pub rate_limit: bool,
    /// 每分钟最大消息数
    #[serde(default = "default_max_messages_per_minute")]
    pub max_messages_per_minute: u32,
    /// 每小时最大消息数
    #[serde(default = "default_max_messages_per_hour")]
    pub max_messages_per_hour: u32,
    /// 信誉分数阈值 (低于此值则限制)
    #[serde(default = "default_reputation_threshold")]
    pub reputation_threshold: f32,
    /// 是否启用自动封禁
    #[serde(default = "default_true")]
    pub auto_ban: bool,
    /// 自动封禁阈值 (触发次数)
    #[serde(default = "default_auto_ban_threshold")]
    pub auto_ban_threshold: u32,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            rate_limit: true,
            max_messages_per_minute: 20,
            max_messages_per_hour: 200,
            reputation_threshold: 0.3,
            auto_ban: true,
            auto_ban_threshold: 10,
        }
    }
}

// ── 配额配置 ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct QuotaConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_segment_minutes")]
    pub segment_minutes: u32,
    #[serde(default = "default_quota_segments")]
    pub segments: Vec<QuotaSegment>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct QuotaSegment {
    pub start_hour: u32,
    pub end_hour: u32,
    pub max_replies: u32,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            segment_minutes: default_segment_minutes(),
            segments: default_quota_segments(),
        }
    }
}

// ── 默认值 ──────────────────────────────────────────────────────

pub(super) fn default_prompts() -> String { "default.txt".into() }
pub(super) fn default_bot_name() -> String { "洛玖".into() }
fn default_frequency_penalty() -> f64 { 2.0 }
fn default_presence_penalty() -> f64 { 1.0 }
fn default_temperature() -> f64 { 1.3 }
fn default_top_p() -> f64 { 0.1 }
fn default_max_tokens() -> u32 { 4096 }
fn default_request_timeout() -> u64 { 60 }
fn default_analysis_max_tokens() -> u32 { 10000 }
fn default_analysis_temperature() -> f64 { 0.3 }
fn default_max_history() -> usize { 10 }
fn default_batch_timeout() -> u64 { 2000 }
fn default_typing_speed() -> f64 { 5.0 }
fn default_max_typing_delay() -> u64 { 4000 }
fn default_reply_follow_up_secs() -> u64 { 300 }
fn default_intrusiveness_weight() -> f32 { 0.3 }
fn default_action_descriptions() -> bool { true }
fn default_reply_cooldown_secs() -> u64 { 15 }
fn default_segment_minutes() -> u32 { 5 }
fn default_quota_segments() -> Vec<QuotaSegment> {
    vec![
        QuotaSegment { start_hour: 0,  end_hour: 6,  max_replies: 0 },
        QuotaSegment { start_hour: 6,  end_hour: 8,  max_replies: 2 },
        QuotaSegment { start_hour: 8,  end_hour: 10, max_replies: 3 },
        QuotaSegment { start_hour: 10, end_hour: 14, max_replies: 5 },
        QuotaSegment { start_hour: 14, end_hour: 16, max_replies: 1 },
        QuotaSegment { start_hour: 16, end_hour: 20, max_replies: 2 },
        QuotaSegment { start_hour: 20, end_hour: 24, max_replies: 1 },
    ]
}
fn default_log_level() -> String { "info".into() }
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
fn default_check_interval() -> u64 { 120 }
fn default_reflection_interval() -> u64 { 1800 }
fn default_max_thoughts() -> usize { 8 }
fn default_post_conversation_delay() -> u64 { 120 }
fn default_concerns_max() -> usize { 5 }
fn default_concern_decay_rate() -> f32 { 0.1 }
fn default_deliberations_max() -> usize { 8 }
fn default_deliberation_decay_rate() -> f32 { 0.05 }
fn default_defect_base_probability() -> f32 { 0.1 }
fn default_max_reply_chars() -> usize { 30 }
fn default_punctuation_style() -> String { "casual".into() }
fn default_style_random_probability() -> f64 { 0.3 }
fn default_vision_base_url() -> String { "https://api.deepseek.com".into() }
fn default_vision_model() -> String { "	deepseek-v4-flash".into() }
fn default_vision_max_tokens() -> u32 { 256 }
fn default_embedding_base_url() -> String { "https://ark.cn-beijing.volces.com/api/v3".into() }
fn default_embedding_model() -> String { "doubao-embedding-vision-251215".into() }
fn default_msg_ok() -> String { "好的".into() }
fn default_msg_already() -> String { "已经开启啦".into() }
fn default_forget_success() -> String { "已遗忘对话记录".into() }
fn default_forget_fail() -> String { "没有找到对话记录".into() }
fn default_max_message_length() -> usize { 2000 }
fn default_sensitive_action() -> String { "warn".into() }
fn default_output_action() -> String { "replace".into() }
fn default_max_messages_per_minute() -> u32 { 20 }
fn default_max_messages_per_hour() -> u32 { 200 }
fn default_reputation_threshold() -> f32 { 0.3 }
fn default_auto_ban_threshold() -> u32 { 10 }
fn default_steal_emoji() -> bool { true }
fn default_max_reg_num() -> usize { 64 }
fn default_do_replace() -> bool { true }

/// 表情包配置
#[derive(Debug, Clone, Deserialize)]
pub struct StickerConfig {
    /// 是否开启自动收集表情包（steal_emoji）
    #[serde(default = "default_steal_emoji")]
    pub steal_emoji: bool,
    /// 非内置表情包最大注册数量
    #[serde(default = "default_max_reg_num")]
    pub max_reg_num: usize,
    /// 超过最大数量时是否自动替换（淘汰最不常用的）
    #[serde(default = "default_do_replace")]
    pub do_replace: bool,
}

impl Default for StickerConfig {
    fn default() -> Self {
        Self {
            steal_emoji: default_steal_emoji(),
            max_reg_num: default_max_reg_num(),
            do_replace: default_do_replace(),
        }
    }
}
