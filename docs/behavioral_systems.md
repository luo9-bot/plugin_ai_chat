# 行为系统文档

## 1. Timing Gate — 何时说话

### 原理
替代原有的 `batch_decide` 硬性规则，用 AI 子代理判断社交语境。

### 流程
1. 消息到达，累积到批处理窗口
2. 危机检测 + @bot 检查（强制回复）
3. 配额检查
4. Timing Gate 子代理评估：
   - 分析"谁在跟谁说话"
   - 判断"bot 是否应该插嘴"
   - 返回 `continue` / `no_reply`

### 工具
- `continue` — 进入回复阶段
- `no_reply` — 沉默等待

### 配置
```yaml
# config.yaml
conversation:
  batch_timeout_ms: 6000  # 批处理窗口
```

### Prompt
`data/plugin_ai_chat/prompts/timing_gate.prompt`

---

## 2. Planner — 多轮推理

### 原理
替代单次 AI 调用，支持多轮工具调用收集信息后再回复。

### 流程
1. Timing Gate 说 `continue`
2. Planner 启动，最多 5 轮：
   - 每轮：AI 调用 → 解析工具 → 执行 → 下一轮
   - 可以查询记忆、查询人物档案
   - 当 AI 调用 `reply` 工具时，触发 Replyer
   - 当 AI 调用 `finish` 工具时，沉默

### 工具
- `reply(reference_info)` — 触发回复生成
- `query_memory(user_id)` — 查询用户记忆
- `query_person_info(user_id)` — 查询人物档案
- `finish(reason)` — 结束推理，不回复

### Prompt
`data/plugin_ai_chat/prompts/planner.prompt`

---

## 3. Replyer — 回复生成

### 原理
从 Planner 分离出来的回复生成逻辑，注入人格/记忆/情绪/表达习惯。

### 流程
1. 接收 Planner 的参考信息
2. 加载表达习惯（从 learner）
3. 构建系统 prompt（replyer 模板 + 上下文）
4. 调用 AI 生成回复
5. 后处理（情绪标签、安全检查、分段）

### Prompt
`data/plugin_ai_chat/prompts/replyer.prompt`

---

## 4. 表达学习 — 从群聊中学习

### 原理
观察群聊消息，提取"情境→表达"对和黑话/梗。

### 流程
1. 每批消息到达后，异步触发学习（间隔 ≥ 30 秒）
2. AI 提取：
   - 表达习惯：当"情境"时，用"风格"
   - 黑话/梗：拼音缩写、英文缩写、中文缩写
3. 去重（相似度 > 0.75 合并）
4. 持久化到 `learner.json`
5. 回复时注入选中的表达习惯

### 数据结构
```rust
ExpressionHabit { situation, style, count, source_group }
JargonEntry { content, jargon_type, meaning, source_group }
```

### Prompt
`data/plugin_ai_chat/prompts/learn_style.prompt`

---

## 5. 回复效果追踪 — ASI 评分

### 原理
发送回复后观察用户反应，评估回复质量。

### 流程
1. 发送回复 → 创建 PENDING 记录
2. 后续用户消息 → 观察 followup
3. 终结触发：
   - 明确负面反馈（"你没懂"、"算了"、"无语"）
   - 修复循环（"我是说"、"你理解错"）
   - 目标用户 2+ 条后续
   - 10 分钟超时
4. 终结 → 计算 ASI 评分

### ASI 评分公式
```
ASI = (0.45 × 行为分 + 0.35 × 关系分 + 0.20 × (1 - 摩擦分)) × 100
```

- 行为分：对话继续(0.30) + 用户情绪(0.25) + 消息长度(0.20) + 无纠正(0.15) + 无放弃(0.10)
- 关系分：社交存在感(0.35) + 温暖度(0.25) + 能力感(0.25) + 恰当性(0.15)
- 摩擦分：明确负面(0.40) + 修复循环(0.30) + 诡异风险(0.30)

---

## 6. 人物档案 — 认识每个人

### 原理
追踪 bot 记住的每个人的信息。

### 数据结构
```rust
PersonProfile {
    user_id, person_name, name_reason,
    know_times, know_since, last_know,
    memory_points, group_nicknames,
}
```

### 集成
- 每次收到消息 → `register_person()`
- Planner 可调用 `query_person_info` 查询
- 回复时注入人物上下文

---

## 7. 危机处理 — 独立模块

### 升级协议
```
None ──[关键词/AI]──→ Mild ──[关键词/AI]──→ Severe
Severe ──[2h + 5条正常]──→ Mild
Mild ──[1h + 3条正常]──→ None
```

### 检测方式
1. 关键词检测（快速）
2. AI 辅助检测（隐晦表达）

### 干预策略
- Mild：温暖倾听，不催促，提及可以找人倾诉
- Severe：立即关心，不空洞安慰，提供热线电话

### 热线电话
- 全国24h心理援助：400-161-9995
- 北京危机干预：010-82951332
- 生命热线：400-821-1215
- 紧急：110 / 120

---

## 8. 配额系统 — 控制回复频率

### 机制
- 时间分段（默认 5 分钟）
- 每段有最大回复数（按时段配置）
- 优先级评分允许突破配额：
  - Darling 在活跃对话：0.45
  - @bot 且 bot 最近活跃：0.20
  - @bot 且 bot 不活跃：0.40
  - 突破阈值：0.45
