# AI Chat Plugin - Luo9 Bot

Rust cdylib 插件，为 Luo9 Bot 提供拟人化 AI 聊天能力。
构建：`cargo build`（默认启用 `plugin` feature）

## 架构概览

消息总线 `luo9_message` -> `handle_group_msg` / `handle_private_msg` -> 批量缓冲 -> Timing Gate -> Planner -> Replyer -> 发送回复

群聊 @bot 消息跳过 Timing Gate 直接回复。
私聊跳过 Planner，直接调用 ai::chat。

## 核心模块

| 目录/文件 | 职责 |
|-----------|------|
| `lib.rs` | 入口 `plugin_main`、状态管理、周期检查 |
| `conversation/` | 消息路由、批处理、上下文构建、命令处理 |
| `timing_gate/` | Timing Gate 决策（continue/no_reply/wait） |
| `planner/` | 多轮推理引擎，Deferred Tool Discovery |
| `replyer/` | 回复生成器，风格随机化 |
| `learner/` | 表达学习系统（情境→表达模式、黑话挖掘） |
| `reply_effect/` | 回复效果追踪 + ASI 评分 + LLM Judge |
| `person_info/` | 人物档案系统（自动回写用户事实） |
| `sticker/` | 洛玖表情包系统（VLM 网格选择、内容过滤） |
| `emoji/` | Unicode emoji 过滤（防止污染记忆） |
| `activity/` | 活动状态系统（训练/吃饭/睡觉等） |
| `conversation_end/` | 对话结束检测（关键词+上下文注入） |
| `crisis/` | 危机处理（检测→升级→干预） |
| `storage/` | 统一 SQLite 存储层 + JSON→SQLite 迁移 |
| `memory/` | 长期记忆（SQLite + BM25 检索 + 向量 embedding） |
| `emotion/` | 情绪系统（10 种状态、时间衰减） |
| `self_memory/` | 自我反思记忆 |
| `mental_state/` | 心理状态（担忧/考量/缺陷） |
| `personality/` | 人格系统（6 套模板、6 维 traits） |
| `working_memory/` | 群聊临时消息（带时间戳、2x 缓存稳定性） |
| `proactive/` | 主动消息（氛围参与、情绪驱动） |
| `quota/` | 回复配额（分时段限制） |
| `schedule/` | 日程计划 |
| `sender/` | 消息发送（分段、打字延迟、emoji 过滤） |
| `vision/` | 识图（Doubao VLM） |
| `anti_injection/` | 防注入（11 个子模块） |
| `admin/` | REST API 管理后台 |
| `typo/` | 错别字生成器（拼音+字频） |
| `util/` | 统一工具函数 |
| `prompt/` | Prompt 管理器 |
| `ai/` | AI API 调用（OpenAI 兼容） |
| `archive/` | 归档 |

## 数据流

```
消息到达
  ├─ 黑名单/防注入检查
  ├─ 人物档案注册
  ├─ 情绪分析
  ├─ 工作记忆记录（带时间戳）
  ├─ 回复效果观察
  ├─ 表情包自动注册（仅 sub_type=1）
  └─ 批处理缓冲 (2s)

批处理
  ├─ 危机检测 → 强制回复
  ├─ @bot 检测 → 强制回复
  ├─ 配额检查
  └─ Timing Gate (AI 子代理)
       ├─ continue → Planner
       ├─ no_reply → 沉默 + 冷却 120s
       └─ wait → 延迟重评估

Planner (群聊多轮，私聊跳过)
  ├─ 工具：reply, query_memory, query_person_info, send_sticker, tool_search, finish
  ├─ Deferred Tool: send_sticker 需要 tool_search 发现
  └─ 最多 10 轮，每轮可查询记忆/人物档案

Replyer (回复生成)
  ├─ 注入人格 + 记忆 + 情绪 + 表达习惯 + 活动状态 + ASI 反馈
  ├─ 风格随机化 (30% 概率切换)
  └─ AI 生成回复

后处理
  ├─ 清理：自记忆标签 + Unicode emoji
  ├─ 输出安全检查
  ├─ 定时任务嵌入
  ├─ 打字延迟发送
  ├─ 回复效果记录
  ├─ 人物事实提取
  └─ 活动状态检测
```

## 上下文注入链

`build_context(user_id, group_id, history)` 组装：
1. 用户标识 + darling_qq 特殊处理（群聊）
2. 自我记忆（bot 内心想法）
3. 用户长期记忆（BM25 检索）
4. 群内其他成员记忆
5. 人格上下文
6. 情绪上下文
7. 心理状态（担忧/考量）
8. 对话状态（关系亲密度）
9. 日程/时间上下文
10. Bot 最近消息
11. 工作记忆（带时间戳，1 小时窗口）
12. 活动状态（训练/吃饭等）
13. ASI 评分反馈（低分时注入调整建议）
14. 缺陷指令
15. 危机干预指令
16. 惩罚上下文

## 数据持久化

统一使用 SQLite（`data/plugin_ai_chat/memory.db`），JSON 文件仅在首次迁移时读取。

| 数据 | 存储方式 |
|------|----------|
| 长期记忆 | SQLite memories 表 |
| 工作记忆 | SQLite working_memories 表 |
| 情绪状态 | SQLite emotions 表 |
| 自我记忆 | SQLite self_memories 表 |
| 人物档案 | SQLite person_profiles 表 |
| 回复效果 | SQLite reply_effects 表 |
| 表达习惯 | SQLite expressions 表 |
| 黑话 | SQLite jargons 表 |
| 表情包 | JSON stickers.json |
| 配额 | JSON quota.json |
| 人格 | JSON personality.json |

## Prompt 系统

所有 prompt 从 `data/plugin_ai_chat/prompts/` 加载，支持 `{placeholder}` 替换。
内置默认嵌入在 `defaults/` 目录，首次运行时自动复制。

## SDK 依赖

`luo9_sdk` v0.6.0：`Bus` 消息总线订阅/发布、`Bot` 消息发送、`Msg` 消息构建。

## 配置要点

- `self_qq`：bot QQ 号
- `admin_qq`：管理员 QQ（0=所有人）
- `darling_qq`：认定的人 QQ（特殊情感反应）
- `conversation.batch_timeout_ms`：消息合并窗口（默认 2s）
- `conversation.reply_cooldown_secs`：回复冷却（默认 15s）
- `vision.base_url`：识图 API 地址（Doubao VLM）
- `embedding.base_url`：Embedding API 地址（多模态向量化）
- `style.reply_style`：默认回复风格
- `style.multiple_reply_styles`：备选风格列表
- `style.style_random_probability`：风格随机概率（默认 0.3）

## 构建说明

- `cargo build`：构建插件（cdylib + rlib）
- `cargo test`：运行测试（62 单元 + 6 集成）
- `cargo clippy`：代码检查
- 前端：`cd frontend && npm run build`（build.rs 自动调用）
- `admin_ui.rs` 由 build.rs 从 `frontend/dist/index.html` 自动生成
