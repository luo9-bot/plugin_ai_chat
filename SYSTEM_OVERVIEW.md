# ai_chat 插件系统总览

## 项目概况

- **语言**：Rust
- **模块数量**：34 个子模块
- **功能定位**：QQ 聊天机器人插件，具备 AI 对话、记忆系统、向量检索、防注入、情绪系统等功能

---

## 已完成系统

### 1. 记忆系统 (`memory/`)

| 子模块 | 文件 | 功能 |
|--------|------|------|
| 多层记忆架构 | `store.rs`, `operations.rs` | 普通记忆、重要记忆、工作记忆 |
| 向量检索 | `embedding.rs`, `vector_store.rs`, `retrieval/` | embedding + 向量存储，语义搜索 |
| 知识图谱 | `graph.rs` | 有向图实体关系存储，Aho-Corasick + LLM 实体提取，Personalized PageRank 重排序 |
| 认知偏差 | `cognitive_biases.rs` | 确认偏误、近因效应、情绪一致性、锚定效应、可得性启发 |
| 不可预测性 | `unpredictability.rs` | 观点漂移、自然遗忘、联想跳跃 |
| 记忆复习 | `review.rs` | 定期回忆机制 |
| 记忆提取 | `extract.rs` | 从对话中提取记忆 |
| 操作日志 | `ops_log.rs` | 记忆操作记录 |

### 2. 防注入系统 (`anti_injection/`)

**多层防御架构**：

| 层级 | 模块 | 功能 |
|------|------|------|
| 字符层 | `unicode.rs`, `normalize.rs` | Unicode 归一化 + confusable skeleton 防绕过 |
| 模式层 | `patterns.rs` | Aho-Corasick 模式引擎 + 否定上下文抑制 |
| 结构层 | `structure.rs` | 结构化注入检测 (JSON/YAML/XML/Markdown/ChatML) |
| 语义层 | `semantic.rs` | 语义启发式扫描 (提示词泄露/元执行/权限覆盖/间接越狱) |
| 评分层 | `scorer.rs` | 贝叶斯风险融合评分 |
| 决策层 | `decision.rs`, `sandbox.rs` | Shadow Sandbox 灰区决策 |
| 行为层 | `behavior.rs` | 用户行为信誉系统 |
| 上下文层 | `context.rs` | 上下文感知 |

### 3. 情绪系统 (`emotion/`)

| 模块 | 功能 |
|------|------|
| `state.rs` | 情绪状态追踪 (EmotionType, CrisisLevel) |
| `detect.rs` | AI 情绪分析、危机检测 |
| `context.rs` | 情绪上下文注入到 prompt |

### 4. 危机处理系统 (`crisis/`)

- 关键词粗筛 + AI 判断的两阶段检测
- 危机等级：None / Mild / Severe
- 无频率限制——真正的危机不会被忽略
- 检查过往对话上下文，判断是否一贯消极倾向

### 5. 人性化系统

#### 5.1 昼夜节律 (`circadian.rs`)

基于正弦曲线的连续模型：

- **精力水平**：早晨上升 → 下午峰值 → 晚上下降 → 深夜最低
- **思维清晰度**：类似精力但略有偏移
- **耐心程度**：下午最低（午後疲倦）
- **社交意愿**：下午和傍晚最高
- **幽默感**：晚上较高

#### 5.2 社交电量 (`social_battery.rs`)

- 电量范围：0.0 - 100.0
- 被动模式消耗 rate×0.1，主动发言消耗 rate×1.5
- 倦怠恢复机制
- 情绪消耗修正（焦虑时消耗更快）

#### 5.3 心灵系统 (`mind/`)

《灵魂架构 v2》的核心：她自己的内心活动不再由旧人格器官（自我反思/心理状态/叙事自我，均已退役）模拟，而是由心灵系统承载：

| 子模块 | 功能 |
|--------|------|
| `stream.rs` | 意识流：append-only，72h 消亡；感官、内心、行动、消化各自入流 |
| `sensation.rs` | 感官：把消息与世界转写为她的第一人称体验 |
| `wake.rs` | 回神：意图堆里只有她自己留下的想起，定时器只兑现不产生意愿；夜间睡眠与睡前整理 |
| `diary.rs` | 日记：睡前整理沉淀下的长存记忆 |
| `persons.rs` | 人物档案：她亲笔维护的对每个人的印象与感受 |
| `recall.rs` | 联想回忆：由感官触发，跨时间召回往事 |
| `security.rs` | 滤壳审计：内心与沉淀入流前的安全事件记录 |

---

### 6. 对话系统（语音管线 `conversation/` + `voice/`）

| 模块 | 功能 |
|------|------|
| `conversation/handler.rs` | 私聊语音路径 + 群聊调度、回复落地簿记 |
| `conversation/batch.rs` | 批次累积与过期分发 |
| `conversation/context.rs` | 场景上下文：状态以第一人称体验注入 |
| `conversation/attention.rs` | 注意力机制 |
| `voice/mod.rs` | **语音合一**：单次 LLM 调用同时完成感知、决策（说话/沉默/表情包）和表达 |
| `ai/tool_loop.rs` | 多轮工具循环：纯文本响应即发言，空响应即沉默 |

### 7. 对话结束检测 (`conversation_end/`)

- **两阶段检测**：
  1. 关键词预筛选（快速，无 AI 调用）
  2. Tool 判断（AI 调用，综合上下文）
- 告别词检测 + 简短确认词检测

---

### 8. 计划系统 (`schedule/`)

| 模块 | 功能 |
|------|------|
| `plan.rs` | 日计划生成与管理 |
| `planner.rs` | 周计划/月计划、目标管理 |
| `config.rs` | 计划配置 |
| `context.rs` | 计划上下文、安静时间判断 |

### 9. 定时任务 (`cron/`)

- 解析 AI 回复中的定时任务请求
- 格式：`{"cron":{"title":"...", "exp":"秒 分 时 日 月 星期 年", "content":"..."}}|cron|回复内容`
- 通过 Bus 注册到任务系统

---

### 10. 表达学习系统 (`learner/`)

| 模块 | 功能 |
|------|------|
| `store.rs` | 表达习惯存储 (ExpressionHabit) |
| `extract.rs` | 从对话中提取表达习惯 |
| `mod.rs` | LLM 子代理选择最合适的表达（候选 ≥ 10 时激活） |

---

### 11. 表情包系统 (`sticker/`)

| 模块 | 功能 |
|------|------|
| `store.rs` | 表情包条目存储 (StickerEntry) |
| `manager.rs` | 注册、选择、发送、维护、淘汰 |
| `mod.rs` | VLM 视觉模型选择、发送接口 |

---

### 12. 管理后台 (`admin/`)

| 模块 | 功能 |
|------|------|
| `ui.rs` | Web 管理界面（Vue 前端嵌入） |
| `handlers.rs` | API 端点：config、memory、anti-injection 等 |
| `backup.rs` | 数据备份 |
| `mod.rs` | HTTP 服务器（tiny_http） |

---

### 13. 安全与加密 (`crypto/`)

- ECC 密钥对生成/加载 (P-256)
- 消息签名/验签
- 公钥 hex 导出

---

### 14. 辅助系统

| 系统 | 模块 | 功能 |
|------|------|------|
| 黑名单/白名单 | `blocklist/` | 用户封禁管理 |
| 配额系统 | `quota/` | 用户配额、分段消息、兴趣追踪 |
| Token 追踪 | `tracking.rs` | API 调用记录、Token 用量统计 |
| 回复效果追踪 | `reply_effect/` | ASI 评分、LLM Judge、观察窗口 |
| 活动生命周期 | `activity/` | 日常生活模拟（训练/吃饭/睡觉/工作/外出/洗澡） |
| 人物档案 | `person_info/` | 用户关系、记忆点、群昵称 |
| 视觉处理 | `vision/` | 图片 CQ 码解析、图片 URL 提取 |
| Emoji 处理 | `emoji/` | Unicode emoji 过滤、纯 emoji 检测 |
| 消息发送 | `sender/` | 分段处理、打字延迟、安全检查 |
| Prompt 管理 | `prompt/` | 外部化 AI prompt，占位符替换，热重载 |
| AI 调用 | `ai/` | Provider、Tools、Types |
| 配置系统 | `config/` | 配置加载/保存、结构体定义、热重载 |
| 状态管理 | `state/` | 共享状态、本地状态 |
| 归档系统 | `archive/` | 工作记忆和长期记忆归档 |
| 工具函数 | `util/` | 时间、文本、JSON 工具 |

---

## 架构特点

1. **语音合一**：群聊/私聊共用一条语音管线，同一个"她"在一次调用里决定说话、沉默或发表情包；沉默是合法输出
2. **状态即体验**：情绪、电量、节律、注意力以第一人称体验句注入 prompt，不以系统指令注入；被通知的状态只能被表演，被体验的状态自然流露
3. **非阻塞主循环**：1ms tick，所有耗时操作在后台线程执行
4. **AI 驱动决策**：危机检测、情绪分析等均使用 AI 判断
5. **多层安全防御**：防注入系统采用 5 层架构，贝叶斯评分 + Shadow Sandbox

---

## 文件统计

- Rust 源文件：115
- 配置结构体字段：200+
- 数据持久化文件：JSON 格式，存储在 `data_dir()`
