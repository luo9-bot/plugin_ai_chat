# AI Chat Plugin - Luo9 Bot

Rust cdylib 插件，为 Luo9 Bot 提供拟人化 AI 聊天能力。
构建：`cargo build`（默认启用 `plugin` feature）

## 架构概览

消息总线 `luo9_message` -> `handle_group_msg` / `handle_private_msg` -> 批量缓冲 -> 过期批次处理 -> `decide_reply` (function call) -> `process_message` -> `ai::chat` -> 发送回复

群聊 @bot 消息跳过 `decide_reply` 直接回复。

## 核心模块

| 文件 | 职责 |
|------|------|
| `lib.rs` | 入口 `plugin_main`、消息路由、批处理、`decide_reply`、`process_message`、`build_context`、周期检查 |
| `ai.rs` | OpenAI 兼容 API 调用：`chat()` 主对话、`analyze_with_tools()` function call 分析、6 个 tool 定义 |
| `config.rs` | YAML 配置反序列化、`init()` 自动生成默认配置、全局 `get()`/`prompt()` |
| `state.rs` | 运行时状态：`UserContext` 对话历史、`MessageBatch` 批量缓冲、回复时间追踪 |
| `memory.rs` | 长期记忆：per-user `MemoryEntry`(Permanent/Important/Normal)、`ai_review_all()` 定期审查 |
| `self_memory.rs` | 自我反思：`SelfThought`(reflection/experience/plan/feeling)、`reflect()` AI 反思 |
| `working_memory.rs` | 群聊临时消息记录，自动过期归档 |
| `personality.rs` | 6 套人格模板、6 维 traits、快照管理 |
| `emotion.rs` | 10 种情绪状态、关键词检测 + AI 分析、时间衰减 |
| `proactive.rs` | 主动消息：AI 生成(情绪驱动/时间问候)、日期提醒、静默时段 |
| `vision.rs` | 识图：CQ 码 URL 提取、Doubao responses API 调用、`[表情包]`/`[照片]` 标注 |
| `sender.rs` | 消息发送、`|^\|` 分段 + 打字延迟模拟 |
| `cron.rs` | AI 回复中嵌入的定时任务解析与注册 |
| `archive.rs` | 过期工作记忆和遗忘的长期记忆归档 |
| `blocklist.rs` | JSON 持久化的用户黑名单 |

## Function Call (Tool Use)

所有结构化分析通过 `ai::analyze_with_tools()` 调用，返回 `serde_json::Value`：

- `decide_reply_tool` -> `{reply: bool, reason: string}` — 群聊回复决策
- `post_analyze_tool` -> `{memories, emotion, corrections}` — 对话后分析（记忆+情绪+纠错）
- `review_conversation_tool` -> `{relevant, emotion}` — 对话审查
- `self_reflect_tool` -> `{thoughts, share}` — 自我反思
- `memory_review_tool` -> `{action, updates, removes, adds}` — 记忆审查
- `proactive_message_tool` -> `{message}` — 主动消息生成

AI 返回 tool_calls 时直接解析 arguments；fallback 到 `extract_json()` 从文本提取。

## 数据持久化

所有数据存 `data/plugin_ai_chat/`：`config.yaml`、`prompts/*.txt`、`memory.json`、`self_memory.json`、`working_memory.json`、`emotion.json`、`personality.json`、`proactive.json`、`proactive_config.json`、`blocklist.json`、`archive.json`

## 上下文注入链

`build_context(user_id, group_id, history)` 组装 system prompt 附加内容：
1. 当前对话用户标识（群聊）
2. 自我记忆（bot 内心想法）
3. 用户长期记忆
4. 群内其他成员记忆
5. 人格上下文
6. 情绪上下文
7. 对话状态（关系亲密度）
8. Bot 最近在群里发的消息
9. 群聊工作记忆

## SDK 依赖

`luo9_sdk` v0.5.1：`Bus` 消息总线订阅/发布、`Bot` 消息发送。禁用 `plugin` feature 时使用 stub 模块。

## 配置要点

- `self_qq`：bot QQ 号，用于群聊 @检测
- `admin_qq`：管理员 QQ（0=所有人都是管理员）
- `conversation.batch_timeout_ms`：消息合并窗口（默认 6s）
- `conversation.reply_follow_up_secs`：对话跟进窗口（默认 300s）
- `vision.base_url`：识图 API 地址（直接拼 `/responses`，不要加路径）
