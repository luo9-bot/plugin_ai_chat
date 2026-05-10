# 危机处理协议

## 检测流程

```
用户消息到达
  │
  ├─ 第一层：关键词检测（快速，无延迟）
  │   ├─ 严重关键词 → CrisisLevel::Severe
  │   ├─ 轻度关键词 → CrisisLevel::Mild
  │   └─ 无命中 → 进入第二层
  │
  └─ 第二层：AI 辅助检测（关键词未命中时）
      ├─ 分析隐晦表达、告别语、隐喻
      ├─ 返回 none / mild / severe
      └─ 无信号 → CrisisLevel::None
```

## 状态机

```
         检测到危机
              │
              ▼
    ┌─────────────────┐
    │   CrisisLevel   │
    │                 │
    │  None ──→ Mild ──→ Severe
    │    ▲        │
    │    │        │
    │    └────────┘
    │   (1h + 3条正常)
    │                 │
    │  Severe ──→ Mild
    │  (2h + 5条正常)
    └─────────────────┘
```

### 升级条件
- 关键词命中：立即升级
- AI 检测到危机：立即升级

### 降级条件
- Severe → Mild：2 小时 + 连续 5 条无危机消息
- Mild → None：1 小时 + 连续 3 条无危机消息
- 自动衰减：长时间无交互时的保底清理

### 干预冷却
- Severe：每次检测到都干预
- Mild：30 分钟内最多干预一次

## 干预策略

### Mild（情绪低落、消极）
- 以角色身份真诚关心
- 认真倾听，不急于给建议
- 让对方知道有人在乎
- 自然提及：如果感觉很不好，可以找信任的人聊聊
- 保持温暖和耐心

### Severe（明确自残/自杀信号）
- 立即表达真诚关心
- 绝不忽略、敷衍、转移话题
- 不说空话（"别想太多"、"会好的"）
- 直接但温柔地回应
- 提供危机热线电话：
  - 全国24h心理援助：400-161-9995
  - 北京危机干预中心：010-82951332
  - 生命热线：400-821-1215
  - 紧急：110 / 120
- 鼓励立即拨打热线或联系信任的人

## 与防注入的交互

- 危机消息豁免防注入的"替换"和"阻止"动作
- 但不豁免色情、暴力、违法内容的检测
- 豁免仅限于情绪危机，不影响安全检测

## 模块接口

```rust
// 检测危机
crisis::detect_crisis(message) -> CrisisLevel
crisis::detect_crisis_ai(message) -> Option<CrisisLevel>

// 更新状态
crisis::update_crisis(user_id, level) -> bool  // 返回是否需要干预

// 获取干预上下文
crisis::get_crisis_context(level) -> String  // 注入到 system prompt

// 通过 emotion 模块的兼容接口
emotion::detect_crisis(message) -> CrisisLevel
emotion::detect_crisis_ai(message) -> Option<CrisisLevel>
emotion::update_crisis(user_id, level) -> bool
emotion::get_crisis_context(level) -> String
```
