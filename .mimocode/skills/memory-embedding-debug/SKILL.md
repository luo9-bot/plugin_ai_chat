---
name: memory-embedding-debug
description: 当遇到内存溢出、embedding 延迟、swap 段被 killed、或消息处理阻塞时使用。专注于 ai_chat 项目内存/embedding 子系统的诊断与优化。
---

# Memory & Embedding System Debug

## Overview

ai_chat 的内存/embedding 子系统是性能问题的高发区域。此 skill 封装了从多个调试 session 中提炼的诊断流程。

**适用场景：**
- 程序因 swap 段溢出被 killed
- 消息处理出现异常延迟（timeout）
- embedding batch 频繁触发导致阻塞
- 长时间运行后内存持续增长
- 搜索/检索耗时异常

## 核心文件清单

诊断时**必须**按顺序审查以下文件：

| 文件 | 职责 |
|------|------|
| `src/memory/mod.rs` | 记忆系统入口，search_memories、push_history |
| `src/memory/embedding.rs` | embedding 生成与批处理 |
| `src/memory/vector_store.rs` | SQ8 量化向量存储、训练、检索 |
| `src/memory/retrieval/vector.rs` | 向量检索逻辑 |
| `src/memory/operations.rs` | 记忆操作（增删改查） |
| `src/memory/ops_log.rs` | 记忆操作日志 |
| `src/memory/review.rs` | 记忆回顾与整理 |
| `src/memory/store.rs` | 底层存储 |
| `src/timing_gate/mod.rs` | 时序门控，控制 AI 调用时机 |
| `src/ai/provider.rs` | AI 调用封装（analyze_with_tools_named 等） |
| `src/lib.rs` | check_periodic 中的后台任务调度 |

## 诊断流程

### Phase 1: 日志分析

用户通常会提供日志文件路径（如 `C:\Users\drluo\Desktop\ttt\bot\ai_chat.log.YYYY-MM-DD`）。

```bash
# 提取所有超时和错误
grep -n "timeout\|WARN\|ERROR\|failed\|killed" <日志路径> | head -50

# 统计超时来源
grep "timeout: global" <日志路径> | awk '{print $4}' | sort | uniq -c | sort -rn

# 查找长时间跳跃（阻塞指标）
# 比较相邻日志行的时间戳，间隔 > 30s 即为可疑
```

### Phase 2: 阻塞点定位

**常见阻塞模式：**

1. **push_history 持锁调用 AI** — 在持有 Mutex/RwLock 时调用 `analyze_with_tools_named`，导致死锁
   - 检查：`push_history` 中是否在锁内调用了 AI
   - 修复：先释放锁，再调用 AI，或使用后台线程

2. **embedding 同步阻塞** — `queue_embedding` 或 `embed_batch` 在主线程同步等待
   - 检查：`embedding.rs` 中是否有 `block_on` 或同步等待
   - 修复：改为后台线程异步执行

3. **search_memories 全量加载** — 加载所有向量到内存进行搜索
   - 检查：`vector_store.rs` 中 `all_vectors()` 是否克隆了全部数据
   - 修复：使用流式/分页搜索，避免全量克隆

4. **train() 清空 write_buffer** — 训练后丢失未持久化数据
   - 检查：`vector_store.rs` 中 `train()` 是否在训练后正确保留 buffer
   - 修复：训练后不清理 write_buffer

5. **config::init() panic** — 配置解析失败导致 abort
   - 检查：配置初始化中的 `expect()` 调用
   - 修复：改为返回 Result 或使用 `unwrap_or` 提供默认值

### Phase 3: 内存优化策略

**关键原则：查询不应占用大量内存，生成可以耗时较长**

- **向量存储**：使用磁盘换内存（非内存空间换时间）
  - 训练后释放 `raw_vectors`，只保留 `quantized_vectors`
  - `search_quantized` 避免临时分配
- **embedding 生成**：后台线程异步执行，不阻塞主流程
- **embedding 检索**：跳过缺失 embedding 的条目，后台补充

### Phase 4: 编译验证循环

```bash
# 编译检查（快速）
cargo build 2>&1 | tail -5

# 完整编译错误（当需要定位编译问题时）
cargo build 2>&1

# 运行测试
cargo test 2>&1 | tail -10

# 检查新警告
cargo build 2>&1 | grep -E "warning.*unused|warning.*move"
```

### Phase 5: 后台任务审查

`src/lib.rs` 中 `check_periodic` 调用的后台任务可能互相阻塞：

```
check_periodic
├── check_proactive_messages   // 主动消息
├── ai_review_all              // 记忆回顾
├── do_self_reflection         // 自我反思
├── do_post_conversation       // 对话后处理
├── run_forgetting_scan        // 遗忘扫描
├── check_activity             // 活跃度检查
├── decay_concerns             // 关注衰减
└── check_and_generate         // 主动生成
```

如果多个后台任务同时调用 AI API，可能导致资源竞争和超时。确保：
- 后台任务使用独立的超时配置
- 任务之间有适当的间隔
- AI 调用不互相阻塞

## 已知修复案例

| 问题 | 根因 | 修复 |
|------|------|------|
| swap 溢出被 killed | `all_vectors()` 全量克隆 | 移除克隆，使用引用 |
| 消息处理延迟 30s+ | embedding 同步阻塞主流程 | 后台线程异步生成 |
| train() 后数据丢失 | 训练清空了 write_buffer | 保留 buffer |
| push_history 死锁 | 持锁调用 AI | 先释放锁再调用 |
| config panic abort | `expect()` 未处理错误 | 改为 Result 返回 |
| embed_batch 日志刷屏 | 每条消息都触发 batch | 降低日志级别，批量合并 |

## 注意事项

- 此 skill 针对 ai_chat 项目的 Rust 代码库，不适用于其他项目
- 诊断时务必结合实际日志，不要仅凭代码猜测
- 修改后必须通过 `cargo build` + `cargo test` 验证
- 内存优化应优先考虑"磁盘换内存"而非"CPU换内存"
