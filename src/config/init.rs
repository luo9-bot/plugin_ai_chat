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

// ── 自动生成默认配置 ────────────────────────────────────────────

pub(super) const DEFAULT_CONFIG_YAML: &str = r#"# ============================================================
#  top.drluo.luo9-ai-chat 配置文件
#  首次运行自动生成，按需修改
# ============================================================

# ── API 配置 (必填) ─────────────────────────────────────────────
api_key: "your_api_key"
base_url: "https://api.deepseek.com/v1"
model: "deepseek-chat"
self_qq: 0                  # 机器人自身 QQ 号 (群消息定向判断必填)
admin_qq: 0                 # 管理员 QQ 号 (控制命令权限，0 = 所有人可用)

# ── 识图功能 (可选) ─────────────────────────────────────────────
# api_key 为空则禁用识图，图片消息会被忽略
vision:
  api_key: ""
  base_url: "https://ark.cn-beijing.volces.com/api/v3"
  model: "doubao-seed-1-8-251228"
  max_tokens: 256

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
  analysis_max_tokens: 10000 # 分析任务最大 token (记忆/情绪/反思分析)
  analysis_temperature: 0.3 # 分析任务温度 (越低越确定)

# ── 对话参数 ─────────────────────────────────────────────────────
conversation:
  max_history: 10           # 对话历史保留轮数
  batch_timeout_ms: 6000    # 消息合并超时 (毫秒)
  typing_speed: 5.0         # 打字模拟速度 (字符/秒)
  max_typing_delay_ms: 4000 # 打字延迟上限 (毫秒)
  reply_follow_up_secs: 300 # 对话跟进超时 (秒)，超时后不再自动回复非@消息
  reply_cooldown_secs: 15  # 对同一用户的回复冷却 (秒)，防止连续回复刷屏
  intrusiveness_weight: 0.3 # 主动插话权重 (0.0~1.0)，越低越容易回复无关消息
  action_descriptions: true # 是否允许括号动作描述，如"（笑了笑）"

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

# ── 自我反思参数 ─────────────────────────────────────────────────
self_reflection:
  interval: 1800             # 自我反思间隔 (秒)，默认 1800 (30分钟)
  max_thoughts: 8            # 注入 prompt 的自我记忆条数 (所有记忆永久保存)
  post_conversation_delay_secs: 120  # 对话结束后多久触发反思 (秒)，默认 120 (2分钟)

# ── 心理状态参数 (缺陷/担忧/考量) ──────────────────────────
mental_state:
  concerns_max: 5             # 最大活跃担忧数
  concern_decay_rate: 0.1     # 担忧衰减速率 (每小时)
  deliberations_max: 8        # 最大活跃考量数
  deliberation_decay_rate: 0.05 # 考量衰减速率 (每小时)
  defect_base_probability: 0.1 # 缺陷基础触发概率

# ── 群聊回复配额 (分时段限制回复次数) ───────────────────────
quota:
  enabled: true                 # 是否启用配额系统
  segment_minutes: 5            # 配额段长度 (分钟)
  segments:                     # 各时段每段最大回复次数
    - {start_hour: 0,  end_hour: 6,  max_replies: 0}   # 深夜不回复
    - {start_hour: 6,  end_hour: 8,  max_replies: 2}   # 早上偏少
    - {start_hour: 8,  end_hour: 10, max_replies: 3}   # 正常
    - {start_hour: 10, end_hour: 14, max_replies: 5}   # 活跃
    - {start_hour: 14, end_hour: 16, max_replies: 1}   # 下午偏少
    - {start_hour: 16, end_hour: 20, max_replies: 2}   # 傍晚中等
    - {start_hour: 20, end_hour: 24, max_replies: 1}   # 晚间话少

# ── 回复风格 ─────────────────────────────────────────────────────
style:
  max_reply_chars: 30         # 单条回复最大字数
  omit_subject: true          # 是否省略主语 ("我觉得无聊" → "无聊")
  punctuation_style: "casual" # 标点风格: casual(不加句号) | formal(正常标点)

# ── 日志配置 ─────────────────────────────────────────────────────
log:
  enabled: true               # 是否启用日志文件
  level: "info"               # 日志级别: debug | info | warn | error

# ── 远程同步 (可选) ─────────────────────────────────────────────
# 将自我记忆同步到远程 MongoDB (通过 memory-viewer API)
# 安全机制: 首次运行自动生成 ECC 密钥对 (data/plugin_ai_chat/ecc_key.pem)
#           所有写入请求通过 ECDSA-P256 签名验证，无需手动配置密钥
sync:
  enabled: false              # 是否启用远程同步
  api_url: ""                 # memory-viewer API 地址，如 "https://xxx.vercel.app"
  db_name: "memory_default"   # 数据库名称 (每个实例应该不同)
  mongodb_uri: ""             # MongoDB 连接字符串，注册时传递给网页端
  display_name: ""            # 显示名称 (暴露时在网页端显示)
  icon: "💭"                  # 显示图标
  expose: false               # 是否向主网页暴露 (允许被其他用户浏览)

# ── 管理后台 (可选) ─────────────────────────────────────────────
# 本地 Web 管理面板，用于管理所有记忆数据
# token 为空则不启动管理后台
admin:
  token: ""                    # 管理员登录令牌 (必填才启用)
  port: 17000                  # 监听端口 (被占用时自动递增)

# ── 防注入配置 (始终开启，不可关闭) ──────────────────────────
# 多层防护系统：输入层、输出层、行为层
# 注意：关键词过滤、注入模式检测、编码绕过检测、色情/暴力/违法内容检测、
# 输出检测 始终强制开启，无法通过配置关闭
anti_injection:
  input:
    max_message_length: 2000   # 最大消息长度
    sensitive_action: "replace"  # 敏感内容处理: replace(替换) | block(阻止)
  output:
    action: "replace"          # 检测到问题时: replace(替换) | block(阻止)
  behavior:
    rate_limit: true           # 频率限制
    max_messages_per_minute: 20  # 每分钟最大消息数
    max_messages_per_hour: 200   # 每小时最大消息数
    reputation_threshold: 0.3  # 信誉分数阈值 (0.0~1.0)
    auto_ban: true             # 自动封禁
    auto_ban_threshold: 10     # 自动封禁触发次数

# ── 用户访问控制 ─────────────────────────────────────────────────
# 白名单：只允许这些用户使用私聊（为空则不限制，全体可用）
# 黑名单：禁止这些用户使用私聊（为空则不限制）
# 两个都配置时，白名单优先（只允许白名单用户，黑名单无效）
# whitelist: []               # 示例: [123456789, 987654321]
# blacklist: []               # 示例: [111111111, 222222222]

# ── 默认启动对话 ─────────────────────────────────────────────────
# 这些用户无需手动发送"开启对话"，启动插件后自动开启私聊
# 也会受到白名单/黑名单的限制
# auto_start_users: []        # 示例: [123456789]

# 这些群无需管理员手动发送"开启对话"，启动插件后自动开启群聊
# auto_start_groups: []       # 示例: [123456789]

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
                base_url: "https://api.deepseek.com/v1".into(),
                model: "deepseek-chat".into(),
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
                messages: Messages::default(),
                log: LogConfig::default(),
                sync: SyncConfig::default(),
                admin: AdminConfig::default(),
                anti_injection: AntiInjectionConfig::default(),
                quota: QuotaConfig::default(),
                whitelist: Vec::new(),
                blacklist: Vec::new(),
                auto_start_users: Vec::new(),
                auto_start_groups: Vec::new(),
            }
        }
    };

    // 提示词文件: 不存在则自动生成
    let prompt_path = data_path.join("prompts").join(&config.prompts);
    if !prompt_path.exists() {
        fs::write(&prompt_path, DEFAULT_PROMPT_TXT).ok();
        debug!(path = ?prompt_path, "generated default prompt");
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
