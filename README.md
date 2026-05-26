# plugin_ai_chat

Luo9 bot AI 插件 - 仅供学习使用。

> **重要法律声明**
>
> 本项目采用 **[GPL v3](LICENSE)** 许可证。使用本软件前，请务必阅读 **[最终用户许可协议 (EULA)](EULA.md)**。
>
> 使用本软件即表示您同意 GPL v3 和 EULA 的全部条款。**不接受则不得使用本软件。**

## 快速开始

### 构建

```bash
cargo build --release
```

### 部署

将 `target/release/plugin_ai_chat.dll`（或 `.so`）放入 Luo9 Bot 插件目录。

### 配置

编辑 `data/plugin_ai_chat/config.yaml`：

```yaml
api_key: "sk-你的密钥"
base_url: "https://api.deepseek.com/v1"
model: "deepseek-chat"
self_qq: 123456789
admin_qq: 123456789
```

## 命令

### 对话控制

群聊中仅管理员可用，私聊中所有用户可用：

| 命令 | 说明 |
|------|------|
| `开!` / `开启对话` | 开启当前群/私聊对话 |
| `停!` / `关闭对话` | 关闭当前群/私聊对话 |
| `遗忘对话` | 查看并清除当前对话历史 |
| `重启对话` | 清除对话历史及所有记忆 |

### 管理员命令

需 `admin_qq` 匹配（设为 0 则所有人可用）：

| 命令 | 说明 |
|------|------|
| `查看群聊` / `查看用户` | 列出已开启的群聊/私聊 |
| `查看黑名单` | 列出所有黑名单用户 |
| `开启群聊:群号` / `关闭群聊:群号` | 远程控制群聊 |
| `开启用户:QQ号` / `关闭用户:QQ号` | 远程控制私聊 |
| `拉黑:QQ号` / `移除黑名单:QQ号` | 黑名单管理 |
| `防注入状态:QQ号` | 查看用户防注入状态 |
| `解封用户:QQ号` / `重置信誉:QQ号` | 封禁/信誉管理 |

### 人格管理

群聊中仅管理员可用，私聊中所有用户可用：

| 命令 | 说明 |
|------|------|
| `人格模板` | 查看可用人格模板列表 |
| `切换人格:温柔体贴` | 切换到指定人格 |
| `调整特质:幽默 0.8` | 调整单项特质 (0.0~1.0) |
| `查看人格` | 查看当前人格详情 |
| `保存人格:备份` / `加载人格:备份` | 人格快照管理 |
| `人格列表` | 查看已保存的快照 |

### 主动对话

| 命令 | 说明 |
|------|------|
| `开启主动对话` / `关闭主动对话` | 控制主动消息推送 |
| `设置免打扰:23-7` | 设置免打扰时段 |
| `提醒我:12-25 圣诞节` | 添加日期提醒 |

### 自然语言记忆管理

- "记住我喜欢喝咖啡" — 永久记忆
- "忘掉我刚才说的" — 清除近期记忆
- "忘掉xxx" — 模糊匹配删除
- "我不叫小明，我叫小红" — 自动修正

## 架构

### 整体数据流

```
消息到达
  ├─ 黑名单/防注入检查
  ├─ 人物档案注册 & 情绪分析
  ├─ 工作记忆记录
  ├─ 表情包自动注册
  └─ 批处理缓冲 (2s)

批处理
  ├─ 危机检测 → 强制回复
  ├─ @bot 检测 → 强制回复
  ├─ 配额检查
  └─ Timing Gate (AI 子代理)
       ├─ continue → Planner
       ├─ no_reply → 沉默 + 冷却
       └─ wait → 延迟重评估

Planner (群聊多轮推理，最多10轮)
  └─ 工具: reply, query_memory, query_person_info,
           send_sticker, tool_search, finish

Replyer (回复生成)
  └─ 注入人格+记忆+情绪+表达习惯+活动状态+ASI反馈

后处理
  ├─ 输出安全检查 & emoji 过滤
  ├─ 打字延迟发送
  ├─ 回复效果记录 & 人物事实提取
  └─ 活动状态检测
```

### 群聊 vs 私聊

| 特性 | 群聊 | 私聊 |
|------|------|------|
| 决策路径 | Planner → Replyer（多轮工具调用） | 直接 `ai::chat` |
| Timing Gate | 冷却/配额/决策 | 不使用 |
| 工作记忆 | 群内所有用户消息流 | 单用户消息流 |
| 主动消息 | 群级冷却 | 需用户回复 |

### 上下文注入链

`build_context` 按序组装 16 层上下文：
系统提示 → 自我记忆 → 长期记忆 → 群成员记忆 → 人格 → 情绪 → 心理状态 → 对话状态 → 日程 → Bot 最近消息 → 工作记忆 → 活动状态 → ASI 反馈 → 缺陷指令 → 危机干预 → 惩罚上下文

## 核心模块

| 模块 | 职责 |
|------|------|
| `conversation/` | 消息路由、批处理、上下文构建、命令处理 |
| `timing_gate/` | 回复时机决策（continue/no_reply/wait） |
| `planner/` | 多轮推理引擎，Deferred Tool Discovery |
| `replyer/` | 回复生成，风格随机化 |
| `memory/` | 长期记忆：SQLite + BM25 + 向量检索 + 知识图谱 |
| `self_memory/` | 自我反思记忆（对话后反思、定期审查、空闲反思） |
| `mental_state/` | 心理状态（担忧/考量/缺陷） |
| `emotion/` | 情绪系统（10 种状态、时间衰减） |
| `personality/` | 人格系统（模板切换、traits 调整、快照管理） |
| `person_info/` | 人物档案（自动回写用户事实） |
| `working_memory/` | 群聊临时消息流（时间戳 + 窗口管理） |
| `proactive/` | 主动消息（氛围参与、情绪驱动） |
| `crisis/` | 危机检测（AI 判断 + 置信度，无频率限制） |
| `reply_effect/` | 回复效果追踪 + ASI 评分 + LLM Judge |
| `learner/` | 表达学习（情境→表达模式、黑话挖掘） |
| `sticker/` | 表情包系统（VLM 网格选择、自动收集、内容过滤） |
| `vision/` | 识图（Doubao VLM） |
| `activity/` | 活动状态（训练/吃饭/睡觉等） |
| `conversation_end/` | 对话结束检测 |
| `schedule/` | 日/周/月计划自动生成与推送 |
| `quota/` | 分时段回复配额 + 兴趣分 |
| `anti_injection/` | 防注入（11 个子模块，多层安全检查） |
| `admin/` | REST API 管理后台 + 前端 UI |
| `typo/` | 错别字生成（拼音+字频） |
| `emoji/` | Unicode emoji 过滤 |
| `blocklist/` | 用户黑名单管理 |
| `ai/` | AI API 调用（OpenAI 兼容协议） |
| `prompt/` | Prompt 模板管理器 |
| `runtime/` | 运行时：工具注册、请求类型、历史截断、去重 |
| `crypto/` | ECC 密钥对管理 |
| `sender/` | 消息发送（分段、打字延迟、emoji 过滤） |
| `config/` | 配置加载与管理 |

## 数据存储

统一使用 SQLite（`data/plugin_ai_chat/memory.db`）：

| 数据 | 表名 |
|------|------|
| 长期记忆 | `memories` |
| 知识图谱 | `graph_nodes` / `graph_edges` |
| 向量索引 | `vectors` |
| 工作记忆 | `working_memories` |
| 情绪状态 | `emotions` |
| 自我记忆 | `self_memories` |
| 人物档案 | `person_profiles` |
| 回复效果 | `reply_effects` |
| 表达习惯 | `expressions` |
| 黑话 | `jargons` |
| 配额 | `quota` |

配置文件：

| 数据 | 格式 |
|------|------|
| 表情包 | JSON（`stickers.json`） |
| 人格快照 | JSON（`personality.json`） |
| 日程计划 | JSON（`daily.json` / `weekly.json` / `monthly.json`） |

## 记忆系统

### 语义检索

双路检索（向量 + BM25） + 后置知识图谱门控 + 自适应阈值 + 智能回退，按用户和群隔离。

### 知识图谱

有向图 + Personalized PageRank 重排序 + BFS 子图扩展 + 关系向量检索，支持 Aho-Corasick + LLM 实体提取。

### 三层写入路径

| 场景 | 落盘位置 |
|------|----------|
| AI 提取 / 对话摘要 | 当前群下 |
| 反思审查 | 被审查群下 |
| 用户说"记住" / 私聊 | 用户全局 |

## 配置要点

```yaml
# ── 核心
model: "deepseek-chat"        # 对话模型
api_key: "sk-xxx"
base_url: "https://api.deepseek.com/v1"

# ── 人格
prompts: "default.txt"        # 人设提示词文件名
bot_name: "洛玖"
darling_qq: 0                 # "认定的人"，有特殊情感反应

# ── 对话
conversation:
  batch_timeout_ms: 2000      # 消息合并窗口
  reply_cooldown_secs: 15     # 回复冷却
  max_history: 10             # 对话历史轮数

# ── 回复
style:
  reply_style: ""             # 默认回复风格
  multiple_reply_styles: []   # 备选风格
  style_random_probability: 0.3
  max_reply_chars: 30

# ── 记忆
memory:
  normal_expire_days: 30
  working_memory_expire_hours: 6

# ── 主动消息
proactive:
  enabled: true
  quiet_start: 23             # 免打扰开始
  quiet_end: 7                # 免打扰结束
  interval: 7200              # 最小间隔 (秒)

# ── 自我反思
self_reflection:
  interval: 1800              # 反思间隔 (秒)
  post_conversation_delay_secs: 120

# ── 识图 (Doubao VLM)
vision:
  api_key: ""
  model: "doubao-1.5-vision-pro-32k"

# ── Embedding
embedding:
  api_key: ""
  model: "doubao-embedding-vision-251215"

# ── 管理后台
admin:
  token: ""                   # 不为空则启动后台
  port: 17000

# ── 访问控制
whitelist: []                 # 私聊白名单
blacklist: []                 # 黑名单
auto_start_users: []          # 自动开启私聊
auto_start_groups: []         # 自动开启群聊
```

## 免责声明

### 作者立场

本插件作者（luoy-oss）**坚决反对**利用本插件进行以下行为：

1. **生成色情内容**：包括但不限于详细性描写、性暗示、色情角色扮演
2. **生成暴力内容**：包括但不限于暴力描述、自残引导、伤害他人
3. **生成违法内容**：包括但不限于毒品、赌博、诈骗、黑客攻击指导
4. **绕过安全限制**：包括但不限于prompt injection、越狱攻击、系统提示泄露
5. **情感操控**：利用AI特性诱导用户产生不健康的情感依赖
6. **侵犯他人权益**：利用AI生成诽谤、骚扰、歧视性内容

### 使用限制

- 本插件仅供**学习研究**和**合规使用**
- 用户应遵守当地法律法规和平台规则
- 用户应自行承担使用本插件产生的一切后果
- 作者不对用户使用本插件进行的任何行为负责

### 内容过滤

本插件内置内容过滤系统，会自动检测和阻止以下内容：
- 色情、暴力、违法内容
- Prompt injection 攻击
- 系统提示泄露尝试
- 恶意角色扮演诱导

**绕过内容过滤系统的行为被视为违反本声明。**

### 技术说明

- 本插件的AI对话功能依赖第三方API服务
- 插件不存储、不传输、不分析用户的个人信息（除对话内容外）
- 所有数据存储在本地，用户可自行管理
- 插件作者无法访问用户的对话内容

### 法律责任

- 用户使用本插件即表示同意本免责声明
- 如不同意本声明，请立即停止使用本插件
- 作者保留随时修改本声明的权利
- 本声明的最终解释权归作者所有

### 版本适用性

**许可覆盖范围**
- 本 README 中的所有条款、使用限制和免责声明，**仅适用于本项目最新发布的发行版**（以下简称"最新版"）。
- "最新版"以本项目 GitHub 仓库的 Releases 页面中标记为 `Latest` 的最新版本为准。

**旧版本使用禁止**
- 作者明确**禁止**任何人以任何目的使用、修改、分发本项目的任何历史版本（旧版本）。
- 所有历史版本均被视为 **"已撤回许可"** 的状态。
- 任何对历史版本的下载、安装、运行或分发行为，均一律被视作**未经授权的使用**，构成对本声明的违反。

**用户义务**
- 使用本软件，即表示您同意并承诺只使用最新版，并已自行将任何现存旧版本替换为最新版。
- 因使用旧版本而产生的一切问题（包括但不限于安全漏洞、内容过滤失效、功能异常、法律风险），作者**不承担任何责任**，全部由使用者自行承担。

---

**最后更新：2026年5月25日**
