# ai_chat — QQ 聊天机器人插件

Rust 编写的 QQ 聊天机器人插件，具备 AI 对话、记忆系统、向量检索、防注入、情绪系统等功能。

## 关键规则

### config.example.yaml 必须与 Config 结构体同步

`save_config_with_comments`（`src/config/access.rs`）基于模板逐行匹配保存配置：
- 注释行会被直接跳过，不进缩进栈也不进 `handled_keys`
- 嵌套字段若被注释，尾部追加逻辑只处理顶级 key，嵌套值会**静默丢失**
- **任何 `src/config/structs.rs` 中 `*Config` 结构体的字段变更，都必须同步更新 `config.example.yaml` 中对应行（确保非注释形式）**

### 阻塞主循环是第二大主题

消息处理主循环是 1ms tick 的非阻塞循环。以下操作必须在后台线程执行：
- embedding 调用（`search_memories` 跳过缺失 embedding，`queue_embedding` 达阈值后台刷新）
- 定时任务（proactive / memory_review / self_reflection 的 AI 调用）
- timing_gate 超时链（`no_error_agent()` 默认 60s 超时，重试 3 次 = 单次最长 3 分钟）

### 内存数据持久化

移除了内存缓存（USER_CACHE / GROUP_CACHE 等），直接读写磁盘。RECENTLY_INJECTED、RECENT_KEYWORD_EXTRACTS、USER_BEHAVIORS 等全部磁盘持久化 + 自动过期清理。

## 项目结构

```
src/
├── admin/          # Web 管理后台 (handlers.rs, ui.rs)
│   └── handlers.rs # 所有 API 端点：config、memory、anti-injection 等
├── anti_injection/ # 防注入系统
├── config/         # 配置加载/保存
│   ├── structs.rs  # 所有配置结构体定义
│   ├── access.rs   # get/reload/save_config_with_comments
│   └── init.rs     # 初始化、DEFAULT_CONFIG_YAML = include_str!("../../config.example.yaml")
├── emotion/        # 情绪系统
├── memory/         # 记忆系统（普通/重要/工作记忆）
├── segments.rs     # 消息分段（|^| 分隔符）
├── timing.rs       # 打字延迟、回复拆分
├── ai.rs           # AI 调用封装
├── handler.rs      # 消息处理入口
└── main.rs         # 插件入口
frontend/
├── src/views/      # Vue 页面组件
│   ├── ConfigView.vue      # 配置编辑（基于 sections 数组定义字段映射）
│   ├── AntiInjectionView.vue # 防注入用户管理
│   └── ...
├── src/api.js      # API 封装（token 认证）
└── dist/           # 构建产物（嵌入 ui.rs）
config.example.yaml # 配置模板，通过 include_str! 嵌入 Rust 二进制
```

## 构建与部署

- **Rust 编译**: `cargo build --release`（`config.example.yaml` 通过 `include_str!` 嵌入）
- **前端构建**: `cd frontend && npm run build`（产物嵌入 `src/admin/ui.rs`）
- 修改 `config.example.yaml` 后需重新编译 Rust 才生效
- 前端修改后需重新构建前端并更新 `ui.rs`

## 已知陷阱

- `config::init()` 中的 `expect` 在 `extern "C"` 函数中 panic 不会刷新日志，改用 `match` + `tracing::error!`
- `handle_group_msg` / `handle_private_msg` 入口处需过滤 `self_qq`，否则 bot 自己的消息会被当作用户消息处理
- `push_history` 持锁调用 AI 会导致死锁，需移到 `std::thread::spawn`
- v1 向量存储用 ID 字符串作 key，重启后全部 miss 导致 embedding 风暴；v2 存原始 content 作 key
- embedding 的 `embed_batch` 有 `MAX_BATCH_SIZE=50` 限制

## 前端配置页面

`ConfigView.vue` 中 `sections` 数组定义了所有可编辑配置项的映射关系。新增配置字段时需同时：
1. 更新 `config.example.yaml`（非注释形式）
2. 更新 `ConfigView.vue` 的 `sections` 数组
3. 确保 `key` 路径与 `structs.rs` 中的嵌套结构一致
