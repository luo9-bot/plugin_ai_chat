# Prompt 系统文档

## PromptManager

所有 AI prompt 从外部 `.prompt` 文件加载，支持 `{placeholder}` 占位符替换。

### API
```rust
// 获取原始模板
let prompt = PromptManager::get().raw("core_rules");

// 替换占位符
let vars = HashMap::from([("name", "value")]);
let rendered = PromptManager::get().render("template_name", &vars);

// 热重载（admin API）
PromptManager::get().reload("template_name")?;

// 列出所有模板
let names = PromptManager::get().list();
```

### 文件位置
- 内置默认：`defaults/*.prompt`（编译时嵌入）
- 运行时覆盖：`data/plugin_ai_chat/prompts/*.prompt`

首次运行时，内置默认会自动复制到 `data/` 目录。用户可修改 `data/` 中的文件覆盖默认。

---

## Prompt 文件列表

| 文件名 | 用途 | 来源 |
|--------|------|------|
| `core_rules` | 核心行为规则（聊天风格、安全规则） | ai.rs |
| `timing_gate` | Timing Gate 决策（何时说话） | 新建 |
| `planner` | Planner 推理（多轮工具调用） | 新建 |
| `replyer` | 回复生成（注入人格/记忆/情绪） | 新建 |
| `post_analyze` | 对话后分析（记忆提取+情绪+纠错） | ai.rs |
| `review_conversation` | 对话审查（群聊记忆提取） | lib.rs |
| `emotion_analyze` | 情绪分析 | emotion.rs |
| `crisis_mild` | 轻度危机干预 | emotion.rs |
| `crisis_severe` | 重度危机干预（含热线电话） | emotion.rs |
| `crisis_ai_detect` | AI 危机检测 | emotion.rs |
| `memory_review` | 记忆审查/合并 | memory.rs |
| `memory_extract` | 记忆提取 | memory.rs |
| `memory_summarize` | 记忆摘要 | memory.rs |
| `self_reflect` | 自我反思 | self_memory.rs |
| `proactive_message` | 主动消息生成 | proactive.rs |
| `mental_state_generate` | 心理状态生成 | mental_state.rs |
| `vision_describe` | 图像描述 | vision.rs |
| `daily_plan` | 日计划生成 | schedule.rs |
| `learn_style` | 表达学习 | 新建 |
| `reply_effect_judge` | 回复效果评判 | 新建 |

---

## 占位符参考

### core_rules
- `{max_reply_chars}` — 最大回复字符数
- `{omit_subject_rule}` — 省略主语规则
- `{punctuation_rule}` — 标点风格规则

### timing_gate
- `{bot_name}` — Bot 名字

### planner
- `{bot_name}` — Bot 名字
- `{identity}` — 人设描述

### replyer
- `{identity}` — 人设描述
- `{reply_style}` — 回复风格
- `{expression_block}` — 表达习惯（运行时注入）
- `{group_chat_attention_block}` — 群聊注意事项

### reply_effect_judge
- `{reply_text}` — 回复内容
- `{followup_messages}` — 后续消息

---

## 如何自定义 Prompt

1. 编辑 `data/plugin_ai_chat/prompts/` 中的 `.prompt` 文件
2. 重启插件（或通过 admin API 热重载）
3. 占位符 `{key}` 会在运行时被替换为实际值

## 如何添加新 Prompt

1. 在 `defaults/` 目录创建新的 `.prompt` 文件
2. 在 `src/prompt/manager.rs` 的 `ensure_defaults()` 中添加条目
3. 在代码中使用 `PromptManager::get().raw("new_name")`
