CREATE TABLE IF NOT EXISTS activation (
    scope      TEXT    NOT NULL,
    id         INTEGER NOT NULL,
    enabled_at INTEGER NOT NULL,
    PRIMARY KEY (scope, id)
);

CREATE TABLE IF NOT EXISTS blocklist (
    user_id    INTEGER PRIMARY KEY,
    reason     TEXT    NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS admin_audit (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    actor      TEXT    NOT NULL,
    command    TEXT    NOT NULL,
    detail     TEXT    NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_admin_audit_created ON admin_audit(created_at);

-- 每个用户一行。state 列是序列化后的 EmotionState：这张表的目的是把
-- 为了读一个用户而解析整份 emotion.json 换成一次主键查询，
-- 而不是把情绪状态的每个字段都拆成列（那会随字段增减不断改 schema）。
CREATE TABLE IF NOT EXISTS emotion (
    user_id    INTEGER PRIMARY KEY,
    state      TEXT    NOT NULL,
    updated_at INTEGER NOT NULL
);

-- 工作记忆：按群隔离的短时消息。`id` 是**稳定身份**——它取代了以前
-- 用写入时间戳当身份的做法（同一秒内的多条消息无法区分，且时间戳会碰撞）。
CREATE TABLE IF NOT EXISTS working_memory (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    group_id    INTEGER NOT NULL,
    user_id     INTEGER NOT NULL,
    content     TEXT    NOT NULL,
    created_at  INTEGER NOT NULL,
    bot_replied INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_working_memory_group ON working_memory(group_id, id);

-- 用量明细：典型追加流，不该按 读 10000 条 + 改 + 写回 的方式维护。
CREATE TABLE IF NOT EXISTS api_usage (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    ts                INTEGER NOT NULL,
    model             TEXT    NOT NULL,
    prompt_name       TEXT    NOT NULL,
    prompt_tokens     INTEGER NOT NULL,
    completion_tokens INTEGER NOT NULL,
    total_tokens      INTEGER NOT NULL,
    cache_hit         INTEGER NOT NULL,
    cache_miss        INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_api_usage_ts ON api_usage(ts);

-- 终身累计：与明细分开，因为明细会被裁剪而累计不会
CREATE TABLE IF NOT EXISTS api_usage_total (
    prompt_name       TEXT PRIMARY KEY,
    calls             INTEGER NOT NULL DEFAULT 0,
    prompt_tokens     INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens      INTEGER NOT NULL DEFAULT 0,
    cache_hit         INTEGER NOT NULL DEFAULT 0,
    cache_miss        INTEGER NOT NULL DEFAULT 0
);

-- 进程级单例状态：**每个子系统一行**。
-- 它们曾经共用一个 cognitive_state.json，而注意力与认知偏差各自
-- 「读整份 → 改自己那一半 → 写回整份」：两次写入交错时，后写的会把
-- 前一次改动整段抹掉（丢失更新）。分表之后各自只碰自己那一行。
CREATE TABLE IF NOT EXISTS cognitive_biases (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    state      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS attention_state (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    state      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- 配额：按 (群, 段) 计数。原先每记一条消息都要重写整份 quota.json
-- （含 48 小时的消息文本），现在只动一行。
CREATE TABLE IF NOT EXISTS quota_segment (
    group_id      INTEGER NOT NULL,
    segment_start INTEGER NOT NULL,
    count         INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (group_id, segment_start)
);

CREATE TABLE IF NOT EXISTS quota_segment_message (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    group_id      INTEGER NOT NULL,
    segment_start INTEGER NOT NULL,
    user_id       INTEGER NOT NULL,
    message       TEXT    NOT NULL,
    ts            INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_quota_message_group_seg
    ON quota_segment_message(group_id, segment_start);
CREATE INDEX IF NOT EXISTS idx_quota_message_ts ON quota_segment_message(ts);

-- 跨天重置用的游标（只有一行）
CREATE TABLE IF NOT EXISTS quota_meta (
    id  INTEGER PRIMARY KEY CHECK (id = 1),
    day TEXT NOT NULL
);

-- 每个用户一行的人物档案 / 关系。
-- 它们曾经分别是 person_info.json 与 relationships.json 里的一整张 map：
-- 读一个人要 clone 全表，写一个人要重写全文件，而两者都在消息路径上。
CREATE TABLE IF NOT EXISTS person (
    user_id    INTEGER PRIMARY KEY,
    profile    TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS relationship (
    user_id    INTEGER PRIMARY KEY,
    state      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Turn 契约的影子观测：只记录「如果强制 Turn tagged union 会怎样」，
-- 不改变任何行为。等分布稳定后再决定是否切换契约。
CREATE TABLE IF NOT EXISTS turn_shadow (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    ts               INTEGER NOT NULL,
    shape            TEXT    NOT NULL,
    valid_under_turn INTEGER NOT NULL,
    detail           TEXT    NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_turn_shadow_ts ON turn_shadow(ts);
