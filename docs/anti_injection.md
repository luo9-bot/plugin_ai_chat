# 防注入系统架构文档

## 概述

ai_chat 插件实现了多层防御的防注入系统，包含 11 个子模块，覆盖输入层、输出层和行为层。

## 模块结构

```
src/anti_injection/
├── mod.rs          # 入口：check_input(), check_output(), init()
├── behavior.rs     # 用户信誉系统、频率限制、自动封禁
├── context.rs      # 跨消息攻击检测、上下文关联
├── decision.rs     # 动作确定、严重度评分
├── normalize.rs    # Unicode 归一化、同形字检测
├── patterns.rs     # Aho-Corasick 加权模式匹配
├── sandbox.rs      # 阴影沙盒决策（灰色地带）
├── scorer.rs       # 贝叶斯风险融合
├── semantic.rs     # 语义启发式扫描
├── structure.rs    # 结构化注入检测（JSON/YAML/XML）
└── unicode.rs      # Unicode 安全、熵检测、混合脚本检测
```

## 检测流程

### 输入层（check_input）

```
用户消息
  │
  ├─ 1. Unicode 归一化（normalize.rs）
  │     - 同形字替换（Cyrillic→Latin, Greek→Latin）
  │     - 零宽字符移除
  │     - 全角→半角转换
  │
  ├─ 2. 模式匹配（patterns.rs）
  │     - Aho-Corasick 多模式匹配
  │     - 加权评分（强模式 vs 弱模式）
  │     - 跨段抑制（防止分段绕过）
  │
  ├─ 3. 结构化注入检测（structure.rs）
  │     - JSON 角色注入检测
  │     - YAML 角色注入检测
  │     - XML 注入检测
  │     - Markdown 围栏注入检测
  │     - ChatML 注入检测
  │
  ├─ 4. 语义扫描（semantic.rs）
  │     - 权威覆盖检测
  │     - 角色抽象检测
  │     - Prompt 泄露检测
  │     - 安全测试越狱检测
  │
  ├─ 5. 信誉系统（behavior.rs）
  │     - 用户信誉分（0.0-1.0）
  │     - 频率限制（每分钟/每小时）
  │     - 违规累积惩罚
  │     - 自动封禁
  │
  ├─ 6. 上下文关联（context.rs）
  │     - 跨消息攻击检测
  │     - 分段注入检测
  │     - 上下文一致性检查
  │
  ├─ 7. 贝叶斯融合（scorer.rs）
  │     - 多维度风险评分融合
  │     - 置信度计算
  │
  ├─ 8. 决策（decision.rs）
  │     - 综合评分 → 动作确定
  │     - Allow / Warn / Replace / Block / Ban
  │
  └─ 9. 阴影沙盒（sandbox.rs）
        - 灰色地带决策
        - 边界案例处理
```

### 输出层（check_output）

AI 生成的回复也经过安全检查：
- 色情/暴力/违法内容检测
- 注入泄露检测（AI 是否泄露了系统 prompt）
- 敏感信息检测

## 关键常量

| 参数 | 默认值 | 说明 |
|------|--------|------|
| max_message_length | 2000 | 最大消息长度 |
| max_messages_per_minute | 20 | 每分钟最大消息数 |
| max_messages_per_hour | 200 | 每小时最大消息数 |
| reputation_threshold | 0.3 | 信誉分阈值 |
| auto_ban_threshold | 10 | 自动封禁触发次数 |

## 配置

```yaml
anti_injection:
  input:
    max_message_length: 2000
    sensitive_action: "block"  # replace | block
  output:
    action: "block"  # replace | block
  behavior:
    rate_limit: true
    max_messages_per_minute: 20
    max_messages_per_hour: 200
    reputation_threshold: 0.3
    auto_ban: true
    auto_ban_threshold: 10
```
