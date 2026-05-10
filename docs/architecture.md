# ai_chat 插件架构文档

## 模块总览

```
src/
├── lib.rs                    # 入口 + 编排 + 状态管理
├── config.rs                 # 配置结构体 + 初始化
├── util.rs                   # 统一时间/JSON/解析工具
│
├── timing_gate/              # Timing Gate 决策（替代 batch_decide）
├── planner/                  # Planner 多轮推理
├── replyer/                  # 回复生成器
├── learner/                  # 表达学习系统
├── reply_effect/             # 回复效果追踪 + ASI 评分
├── person_info/              # 人物档案系统
├── prompt/                   # Prompt 管理系统
├── crisis/                   # 危机处理模块
│
├── ai/                       # AI API 调用
├── emotion/                  # 情绪系统
├── memory/                   # 用户长期记忆
├── self_memory/              # Bot 自我反思记忆
├── mental_state/             # 心理状态（担忧/领悟/缺陷）
├── personality/              # 人格系统
├── working_memory/           # 群聊临时消息记录
├── proactive/                # 主动消息
├── quota/                    # 回复配额
├── schedule/                 # 日程计划
├── sender/                   # 消息发送
├── vision/                   # 图像识别
├── admin/                    # 管理 REST API
└── anti_injection/           # 防注入（多层防御）
```

## 数据流

```
消息到达
  │
  ├─ 黑名单/防注入检查
  ├─ 记录人物档案 (person_info)
  ├─ 情绪分析 (emotion)
  ├─ 工作记忆记录 (working_memory)
  ├─ 回复效果观察 (reply_effect)
  │
  ▼
消息批处理 (batch_timeout_ms 窗口)
  │
  ├─ 危机检测 (crisis) → 强制回复
  ├─ @bot 检测 → 强制回复
  ├─ 配额检查 (quota)
  │
  ▼
Timing Gate (timing_gate)
  │
  ├─ no_reply → 沉默
  ├─ continue → 进入 Planner
  │
  ▼
Planner (planner) — 多轮工具调用
  │
  ├─ query_memory → 查询记忆
  ├─ query_person_info → 查询人物档案
  ├─ reply → 触发 Replyer
  ├─ finish → 沉默
  │
  ▼
Replyer (replyer) — 生成回复文本
  │
  ├─ 注入表达习惯 (learner)
  ├─ 注入人格/记忆/情绪
  ├─ 调用 AI 生成
  │
  ▼
后处理
  │
  ├─ 情绪标签解析
  ├─ 输出安全检查 (anti_injection)
  ├─ 定时任务处理 (cron)
  ├─ 打字延迟发送 (sender)
  │
  ▼
记录回复效果 (reply_effect)
  │
  └─ 表达学习 (learner) — 异步后台
```

## 状态管理

### 全局状态 (static)
- `PROCESSING_USERS` — 防止并发处理同一用户
- `MESSAGE_QUEUE` — 消息处理队列
- `SHARED_STATE` — 跨线程共享状态（对话历史、回复时间）
- `LAST_*_CHECK` — 周期任务时间戳

### 线程状态 (thread_local)
- `STATE` — 活跃用户/群聊、黑名单、消息批次

### 持久化状态 (JSON 文件)
- `memory.json` — 用户长期记忆
- `emotion.json` — 用户情绪状态
- `self_memory.json` — Bot 自我反思
- `mental_state.json` — 心理状态
- `personality.json` — 人格配置
- `working_memory.json` — 群聊工作记忆
- `proactive.json` — 主动消息状态
- `person_info.json` — 人物档案
- `learner.json` — 表达习惯库
- `reply_effects.json` — 回复效果记录
- `quota.json` — 配额计数
- `daily_plan.json` — 日计划
- `blocklist.json` — 黑名单
- `archive.json` — 归档数据
