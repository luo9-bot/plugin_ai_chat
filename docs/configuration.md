# 配置参考文档

## 配置文件位置

`data/plugin_ai_chat/config.yaml`

## 配置项总览

### 基础配置

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| api_key | string | 必填 | OpenAI 兼容 API Key |
| base_url | string | 必填 | API 地址（如 `https://api.deepseek.com`） |
| model | string | 必填 | 模型名称（如 `deepseek-chat`） |
| self_qq | u64 | 0 | Bot QQ 号（0=回复所有消息） |
| admin_qq | u64 | 0 | 管理员 QQ（0=所有人可用） |
| darling_qq | u64 | 0 | 认定的人 QQ（0=不启用） |
| prompts | string | "default.txt" | 提示词文件名 |

### AI 调用参数

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| ai.frequency_penalty | f64 | 2.0 | 频率惩罚 (-2.0~2.0) |
| ai.presence_penalty | f64 | 1.0 | 存在惩罚 (-2.0~2.0) |
| ai.temperature | f64 | 1.3 | 温度 (0.0~2.0) |
| ai.top_p | f64 | 0.1 | 核采样 (0.0~1.0) |
| ai.max_tokens | u32 | 4096 | 最大回复 token |
| ai.request_timeout | u64 | 60 | 请求超时（秒） |
| ai.analysis_max_tokens | u32 | 10000 | 分析任务最大 token |
| ai.analysis_temperature | f64 | 0.3 | 分析任务温度 |

### 对话参数

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| conversation.max_history | usize | 10 | 对话历史保留轮数 |
| conversation.batch_timeout_ms | u64 | 2000 | 消息合并超时（毫秒） |
| conversation.typing_speed | f64 | 5.0 | 打字模拟速度（字符/秒） |
| conversation.max_typing_delay_ms | u64 | 4000 | 最大打字延迟（毫秒） |
| conversation.reply_follow_up_secs | u64 | 300 | 对话跟进超时（秒） |
| conversation.reply_cooldown_secs | u64 | 15 | 回复冷却（秒） |
| conversation.intrusiveness_weight | f32 | 0.3 | 主动插话权重 (0.0~1.0) |
| conversation.action_descriptions | bool | true | 是否允许动作描述 |

### 记忆参数

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| memory.normal_expire_days | u64 | 30 | 普通记忆过期天数 |
| memory.important_fade_days | u64 | 7 | 重要记忆降权天数 |
| memory.auto_summarize_threshold | usize | 10 | 自动摘要触发阈值 |
| memory.working_memory_expire_hours | u64 | 6 | 工作记忆过期时间 |

### 情绪参数

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| emotion.decay_rate | f64 | 0.15 | 情绪衰减速率（每小时） |
| emotion.decay_delay_secs | u64 | 60 | 衰减延迟（秒） |
| emotion.neutral_threshold | f64 | 0.15 | 平静恢复阈值 |
| emotion.affinity_threshold | f64 | 3.0 | 亲近感阈值（次/小时） |

### 主动对话参数

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| proactive.enabled | bool | true | 是否启用 |
| proactive.quiet_start | u32 | 23 | 免打扰开始（24h） |
| proactive.quiet_end | u32 | 7 | 免打扰结束 |
| proactive.interval | u64 | 7200 | 主动消息间隔（秒） |
| proactive.max_ignore | u32 | 3 | 忽略次数上限 |
| proactive.low_mood_multiplier | f64 | 2.0 | 低情绪间隔倍率 |
| proactive.check_interval | u64 | 60 | 周期检查间隔（秒） |

### 回复风格

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| style.max_reply_chars | usize | 30 | 单条回复最大字数 |
| style.omit_subject | bool | true | 是否省略主语 |
| style.punctuation_style | string | "casual" | 标点风格 |
| style.reply_style | string | "" | 默认回复风格 |
| style.multiple_reply_styles | Vec | [] | 备选风格列表 |
| style.style_random_probability | f64 | 0.3 | 使用备选风格概率 |

### 识图功能

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| vision.api_key | string | "" | 识图 API Key（空=禁用） |
| vision.base_url | string | "" | 识图 API 地址 |
| vision.model | string | "" | 识图模型 |
| vision.max_tokens | u32 | 256 | 最大 token |

### 日志配置

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| log.enabled | bool | true | 是否启用日志 |
| log.level | string | "info" | 日志级别 |

### 远程同步

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| sync.enabled | bool | false | 是否启用 |
| sync.api_url | string | "" | API 地址 |
| sync.db_name | string | "memory_default" | 数据库名 |

### 管理后台

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| admin.token | string | "" | 登录令牌（空=不启用） |
| admin.port | u16 | 17000 | 监听端口 |

### 配额系统

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| quota.enabled | bool | true | 是否启用 |
| quota.segment_minutes | u64 | 5 | 配额段长度（分钟） |
| quota.segments | Vec | 7 个时段 | 各时段配额 |
