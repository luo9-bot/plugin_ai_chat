use serde::{Deserialize, Serialize};
use std::fs;
use tracing::{debug, info};

use crate::config;
use crate::emotion;


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ThoughtCategory {
    Reflection,
    Experience,
    Plan,
    Feeling,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfThought {
    pub content: String,
    pub category: ThoughtCategory,
    pub created: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SelfMemoryStore {
    pub thoughts: Vec<SelfThought>,
}

/// 群组画像：AI 用来判断往哪个群分享
pub struct GroupProfile {
    pub group_id: u64,
    pub recent_messages: String,  // 格式化的最近消息摘要
}

fn store_path() -> std::path::PathBuf {
    config::data_dir().join("self_memory.json")
}

impl SelfMemoryStore {
    fn load() -> Self {
        let path = store_path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self) {
        let path = store_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            fs::write(path, json).ok();
        }
    }
}

/// 初始化时调用：返回自我记忆条数
pub fn load_count() -> usize {
    let store = SelfMemoryStore::load();
    store.thoughts.len()
}

/// 添加一条自我思考 (永久保存，不会自动淘汰)
/// 自动去重：如果已有高度相似的想法，跳过
pub fn add(content: &str, category: ThoughtCategory) {
    let mut store = SelfMemoryStore::load();

    // 去重：检查是否已有相似内容
    let normalized = normalize_thought(content);
    for existing in &store.thoughts {
        let existing_norm = normalize_thought(&existing.content);
        if is_similar(&normalized, &existing_norm) {
            debug!(content, "self_memory: skipped duplicate thought");
            return;
        }
    }

    let now = crate::util::now_secs();
    debug!(content, ?category, "self_memory: added thought");
    store.thoughts.push(SelfThought {
        content: content.to_string(),
        category,
        created: now,
    });
    store.save();

    // 远程同步 (fire-and-forget)
    if crate::config::get().sync.enabled {
        if let Some(thought) = store.thoughts.last() {
            sync_to_remote(thought);
        }
    }
}

/// 标准化想法文本用于比较
fn normalize_thought(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c >= '\u{4e00}' && *c <= '\u{9fff}')
        .collect::<String>()
        .to_lowercase()
}

/// 检查两个标准化后的想法是否高度相似
/// 使用最长公共子序列(LCS)比例判断
fn is_similar(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }

    // 完全相同
    if a == b {
        return true;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let shorter_len = a_chars.len().min(b_chars.len());

    // 子串包含
    let (shorter, longer) = if a_chars.len() <= b_chars.len() { (&a_chars, &b_chars) } else { (&b_chars, &a_chars) };
    if longer.len() >= 6 && longer.windows(shorter.len()).any(|w| w == shorter.as_slice()) {
        return true;
    }

    // LCS 比例: 最长公共子序列占较短文本的 50% 以上
    if shorter_len >= 6 {
        let lcs = lcs_len(&a_chars, &b_chars);
        if lcs as f64 / shorter_len as f64 > 0.5 {
            return true;
        }
    }

    false
}

/// 最长公共子序列长度
fn lcs_len(a: &[char], b: &[char]) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    if a_len == 0 || b_len == 0 { return 0; }
    let mut prev = vec![0usize; b_len + 1];
    for i in 1..=a_len {
        let mut curr = vec![0usize; b_len + 1];
        for j in 1..=b_len {
            if a[i - 1] == b[j - 1] {
                curr[j] = prev[j - 1] + 1;
            } else {
                curr[j] = prev[j].max(curr[j - 1]);
            }
        }
        prev = curr;
    }
    prev[b_len]
}

/// 总共保存了多少条自我记忆
pub fn total_count() -> usize {
    let store = SelfMemoryStore::load();
    store.thoughts.len()
}

/// 修正自我记忆：根据 old 模糊匹配，替换为 new (new 为空则删除)
/// 返回修正的条数
pub fn correct(old: &str, new: &str) -> usize {
    let mut store = SelfMemoryStore::load();
    let mut count = 0;

    if new.is_empty() {
        let before = store.thoughts.len();
        store.thoughts.retain(|t| !t.content.contains(old));
        count = before - store.thoughts.len();
    } else {
        for thought in &mut store.thoughts {
            if thought.content.contains(old) {
                thought.content = new.to_string();
                count += 1;
            }
        }
    }

    if count > 0 {
        store.save();
        debug!(old, new, count, "self_memory: corrected entries");
    }
    count
}

/// 获取最近的自我思考上下文 (注入到 system prompt)
pub fn get_context(max_count: usize) -> String {
    let store = SelfMemoryStore::load();
    if store.thoughts.is_empty() {
        return String::new();
    }

    let lines: Vec<String> = store.thoughts
        .iter()
        .rev()
        .take(max_count)
        .map(|t| {
            let cat = match t.category {
                ThoughtCategory::Reflection => "反思",
                ThoughtCategory::Experience => "经历",
                ThoughtCategory::Plan => "计划",
                ThoughtCategory::Feeling => "感受",
            };
            format!("- [{}] {}", cat, t.content)
        })
        .collect();

    format!("# 你最近的想法\n{}", lines.join("\n"))
}

/// AI 驱动的自我反思
///
/// recent_context: 最近的对话上下文文本
/// group_profiles: 各群的画像 (群号 → 最近消息摘要)，AI 用来判断往哪个群分享
///
/// 返回: (thoughts_added, share_info)
pub fn reflect(
    recent_context: &str,
    group_profiles: &[GroupProfile],
) -> (usize, Option<(String, u64)>) {
    info!("self_reflect: 开始自我反思");
    // 构建反思上下文
    let mut context_parts = Vec::new();

    // 用户 prompt (人设定义)
    let user_prompt = config::prompt();
    if !user_prompt.is_empty() {
        context_parts.push(user_prompt.to_string());
    }

    // 人格信息
    let personality = crate::personality::get_prompt_context();
    if !personality.is_empty() {
        context_parts.push(personality);
    }

    // 情绪状态 (取一个代表性的)
    let emotion_ctx = emotion::get_prompt_context(0);
    if !emotion_ctx.is_empty() {
        context_parts.push(emotion_ctx);
    }

    // 最近的自我思考 (避免重复)
    let existing = get_context(20);
    if !existing.is_empty() {
        context_parts.push(existing);
    }

    // 最近的对话
    if !recent_context.is_empty() {
        context_parts.push(format!("# 最近的对话\n{}", recent_context));
    }

    // 各群的画像 (让 AI 了解每个群是干什么的)
    if !group_profiles.is_empty() {
        let profiles_text: Vec<String> = group_profiles.iter().map(|p| {
            format!("## 群{}\n{}", p.group_id, p.recent_messages)
        }).collect();
        context_parts.push(format!("# 你所在的群\n{}", profiles_text.join("\n\n")));
    }

    let full_context = context_parts.join("\n\n");

    match crate::ai::analyze_with_tools(
        crate::prompt::PromptManager::get().raw("self_reflect"),
        &full_context,
        &[crate::ai::self_reflect_tool()],
        Some(serde_json::json!("auto"))
    ) {
        Ok(parsed) => {
            // 解析 thoughts
            let mut count = 0;
            if let Some(thoughts) = parsed.get("thoughts").and_then(|v| v.as_array()) {
                for thought in thoughts {
                    let content = thought.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    if content.is_empty() {
                        continue;
                    }
                    let category = match thought.get("category").and_then(|v| v.as_str()).unwrap_or("") {
                        "experience" => ThoughtCategory::Experience,
                        "plan" => ThoughtCategory::Plan,
                        "feeling" => ThoughtCategory::Feeling,
                        _ => ThoughtCategory::Reflection,
                    };
                    add(content, category);
                    count += 1;
                }
            }

            // 处理主动分享

            // 解析担忧
            if let Some(concerns) = parsed.get("concerns").and_then(|v| v.as_array()) {
                for item in concerns {
                    let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    let category = item.get("category").and_then(|v| v.as_str()).unwrap_or("social");
                    if !content.is_empty() {
                        crate::mental_state::add_concern(content, category, 0, 0);
                    }
                }
            }
            // 解析考量
            if let Some(deliberations) = parsed.get("deliberations").and_then(|v| v.as_array()) {
                for item in deliberations {
                    let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    if !content.is_empty() {
                        crate::mental_state::add_deliberation(content, "reflection");
                    }
                }
            }

            // 处理主动分享
            let share = parsed.get("share").and_then(|s| {
                let should_share = s.get("should_share").and_then(crate::ai::parse_bool).unwrap_or(false);
                let content = s.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let target = s.get("target_group_id").and_then(|v| v.as_u64()).unwrap_or(0);
                if should_share && !content.is_empty() && target > 0 {
                    Some((content.to_string(), target))
                } else {
                    None
                }
            });

            if count > 0 {
                debug!(count, "self_reflect: added thoughts");
            }

            (count, share)
        }
        Err(e) => {
            debug!(error = %e, "self_reflect: AI error");
            (0, None)
        }
    }
}

// ── 远程同步 ────────────────────────────────────────────────────

fn api_url(path: &str) -> String {
    let base = &config::get().sync.api_url;
    format!("{}/api/thoughts{}", base.trim_end_matches('/'), path)
}

fn db_name() -> String {
    config::get().sync.db_name.clone()
}

/// 生成签名 headers (x-db-name, x-timestamp, x-signature)
fn sign_headers() -> [(String, String); 3] {
    let db = db_name();
    let ts = crate::util::now_secs().to_string();
    let message = format!("{}:{}", db, ts);
    let signature = crate::crypto::sign_message(&message);
    [
        ("x-db-name".to_string(), db),
        ("x-timestamp".to_string(), ts),
        ("x-signature".to_string(), signature),
    ]
}

/// 将单条想法同步到远程 (fire-and-forget)
fn sync_to_remote(thought: &SelfThought) {
    let body = serde_json::json!({
        "db_name": db_name(),
        "thoughts": [{
            "content": thought.content,
            "category": serde_json::to_value(&thought.category).unwrap_or_default(),
            "created": thought.created,
        }]
    });
    let json = body.to_string();
    let headers = sign_headers();

    let mut req = ureq::post(&api_url("/bulk-sync"))
        .header("Content-Type", "application/json");
    for (k, v) in &headers {
        req = req.header(k, v);
    }

    match req.send(json.as_bytes()) {
        Ok(_) => debug!("sync_to_remote: ok"),
        Err(e) => info!("sync_to_remote: error {}", e),
    }
}

/// 向注册表注册自己 (启动时调用，如果 expose=true)
pub fn register_to_registry() {
    let cfg = config::get();
    if !cfg.sync.enabled || !cfg.sync.expose {
        return;
    }
    let display_name = if cfg.sync.display_name.is_empty() {
        format!("memory_{}", cfg.sync.db_name)
    } else {
        cfg.sync.display_name.clone()
    };
    let icon = if cfg.sync.icon.is_empty() {
        "💭".to_string()
    } else {
        cfg.sync.icon.clone()
    };

    info!(db_name = %cfg.sync.db_name, display_name = %display_name, "register_to_registry: 注册中");

    let body = serde_json::json!({
        "db_name": cfg.sync.db_name,
        "display_name": display_name,
        "icon": icon,
        "mongodb_uri": cfg.sync.mongodb_uri,
        "public_key": crate::crypto::public_key_hex(),
    });
    let json = body.to_string();
    let url = format!("{}/api/registry", cfg.sync.api_url.trim_end_matches('/'));
    let headers = sign_headers();

    let mut req = ureq::post(&url)
        .header("Content-Type", "application/json");
    for (k, v) in &headers {
        req = req.header(k, v);
    }

    match req.send(json.as_bytes())
    {
        Ok(_) => info!("register_to_registry: 注册成功"),
        Err(e) => info!("register_to_registry: 注册失败 {}", e),
    }
}

/// 将全部本地想法同步到远程
pub fn sync_all_to_remote() -> Result<usize, String> {
    let store = SelfMemoryStore::load();
    let total = store.thoughts.len();
    if total == 0 {
        return Err("本地没有想法".into());
    }

    info!(total, "sync_all_to_remote: 开始同步");

    let thoughts: Vec<serde_json::Value> = store.thoughts.iter().map(|t| {
        serde_json::json!({
            "content": t.content,
            "category": serde_json::to_value(&t.category).unwrap_or_default(),
            "created": t.created,
        })
    }).collect();

    let body = serde_json::json!({
        "db_name": db_name(),
        "thoughts": thoughts,
    });
    let json = body.to_string();
    let headers = sign_headers();
    debug!(bytes = json.len(), "sync_all_to_remote: 请求体准备完成");

    let url = api_url("/bulk-sync");
    debug!(url = %url, "sync_all_to_remote: 发送请求");

    let mut req = ureq::post(&url)
        .header("Content-Type", "application/json");
    for (k, v) in &headers {
        req = req.header(k, v);
    }

    let mut resp = req.send(json.as_bytes())
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = resp.status().as_u16();
    debug!(status, "sync_all_to_remote: 收到响应");

    let resp_str = resp.body_mut().read_to_string()
        .map_err(|e| format!("读取响应失败: {}", e))?;
    let resp_json: serde_json::Value = serde_json::from_str(&resp_str)
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let inserted = resp_json.get("inserted").and_then(|v: &serde_json::Value| v.as_u64()).unwrap_or(0);
    info!(total, inserted, "sync_all_to_remote: 同步完成");
    Ok(inserted as usize)
}

/// 获取远程全部记忆 (用于 pull 同步)
pub fn remote_list_all() -> Result<serde_json::Value, String> {
    debug!("remote_list_all: 查询中");

    let url = format!("{}?db={}", api_url(""), db_name());

    let mut resp = ureq::get(&url)
        .call()
        .map_err(|e| format!("请求失败: {}", e))?;

    let resp_str = resp.body_mut().read_to_string()
        .map_err(|e| format!("读取响应失败: {}", e))?;
    let resp_json: serde_json::Value = serde_json::from_str(&resp_str)
        .map_err(|e| format!("解析响应失败: {}", e))?;

    debug!("remote_list_all: 查询完成");
    Ok(resp_json)
}

/// 关键词搜索远程记忆
pub fn remote_search(keyword: &str) -> Result<serde_json::Value, String> {
    debug!(keyword, "remote_search: 搜索中");

    let body = serde_json::json!({
        "db_name": db_name(),
        "keyword": keyword,
    });
    let json = body.to_string();
    let headers = sign_headers();

    let mut req = ureq::post(&api_url("/search"))
        .header("Content-Type", "application/json");
    for (k, v) in &headers {
        req = req.header(k, v);
    }

    let mut resp = req.send(json.as_bytes())
        .map_err(|e| format!("请求失败: {}", e))?;

    let resp_str = resp.body_mut().read_to_string()
        .map_err(|e| format!("读取响应失败: {}", e))?;
    let resp_json: serde_json::Value = serde_json::from_str(&resp_str)
        .map_err(|e| format!("解析响应失败: {}", e))?;

    debug!(keyword, "remote_search: 搜索完成");
    Ok(resp_json)
}

/// 关键词批量软删除远程记忆
pub fn remote_search_delete(keyword: &str) -> Result<u64, String> {
    info!(keyword, "remote_search_delete: 开始删除");

    let body = serde_json::json!({
        "db_name": db_name(),
        "keyword": keyword,
        "action": "delete",
    });
    let json = body.to_string();
    let headers = sign_headers();

    let mut req = ureq::post(&api_url("/search"))
        .header("Content-Type", "application/json");
    for (k, v) in &headers {
        req = req.header(k, v);
    }

    let mut resp = req.send(json.as_bytes())
        .map_err(|e| format!("请求失败: {}", e))?;

    let resp_str = resp.body_mut().read_to_string()
        .map_err(|e| format!("读取响应失败: {}", e))?;
    let resp_json: serde_json::Value = serde_json::from_str(&resp_str)
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let deleted = resp_json.get("deleted").and_then(|v: &serde_json::Value| v.as_u64()).unwrap_or(0);
    info!(keyword, deleted, "remote_search_delete: 完成");
    Ok(deleted)
}

/// 按 ID 删除单条远程记忆 (软删除)
pub fn remote_delete(id: &str) -> Result<(), String> {
    info!(id, "remote_delete: 删除中");

    let body = serde_json::json!({
        "db_name": db_name(),
    });
    let json = body.to_string();
    let headers = sign_headers();

    let mut req = ureq::delete(&api_url(&format!("/{}", id)))
        .header("Content-Type", "application/json");
    for (k, v) in &headers {
        req = req.header(k, v);
    }

    req.force_send_body().send(json.as_bytes())
        .map_err(|e| format!("请求失败: {}", e))?;

    info!(id, "remote_delete: 删除成功");
    Ok(())
}

/// 恢复远程已删除的记忆
pub fn remote_restore(id: &str) -> Result<(), String> {
    info!(id, "remote_restore: 恢复中");

    let body = serde_json::json!({
        "action": "restore",
        "db_name": db_name(),
    });
    let json = body.to_string();
    let headers = sign_headers();

    let mut req = ureq::patch(&api_url(&format!("/{}", id)))
        .header("Content-Type", "application/json");
    for (k, v) in &headers {
        req = req.header(k, v);
    }

    req.send(json.as_bytes())
        .map_err(|e| format!("请求失败: {}", e))?;

    info!(id, "remote_restore: 恢复成功");
    Ok(())
}

/// 获取远程已删除记忆列表
pub fn remote_list_deleted() -> Result<serde_json::Value, String> {
    debug!("remote_list_deleted: 查询中");

    let url = format!("{}?db={}&include_deleted=true&deleted_only=true",
        api_url(""), db_name());

    let mut resp = ureq::get(&url)
        .call()
        .map_err(|e| format!("请求失败: {}", e))?;

    let resp_str = resp.body_mut().read_to_string()
        .map_err(|e| format!("读取响应失败: {}", e))?;
    let resp_json: serde_json::Value = serde_json::from_str(&resp_str)
        .map_err(|e| format!("解析响应失败: {}", e))?;

    debug!("remote_list_deleted: 查询完成");
    Ok(resp_json)
}

/// 清理远程超过30天的已删除记忆
pub fn remote_purge() -> Result<u64, String> {
    info!("remote_purge: 开始清理过期记忆");

    let body = serde_json::json!({
        "db_name": db_name(),
    });
    let json = body.to_string();
    let headers = sign_headers();

    let mut req = ureq::post(&api_url("/purge"))
        .header("Content-Type", "application/json");
    for (k, v) in &headers {
        req = req.header(k, v);
    }

    let mut resp = req.send(json.as_bytes())
        .map_err(|e| format!("请求失败: {}", e))?;

    let resp_str = resp.body_mut().read_to_string()
        .map_err(|e| format!("读取响应失败: {}", e))?;
    let resp_json: serde_json::Value = serde_json::from_str(&resp_str)
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let purged = resp_json.get("purged").and_then(|v: &serde_json::Value| v.as_u64()).unwrap_or(0);
    info!(purged, "remote_purge: 清理完成");
    Ok(purged)
}

/// 获取远程记忆分类统计
pub fn remote_stats() -> Result<serde_json::Value, String> {
    debug!("remote_stats: 查询中");

    let url = format!("{}/stats?db={}", api_url(""), db_name());

    let mut resp = ureq::get(&url)
        .call()
        .map_err(|e| format!("请求失败: {}", e))?;

    let resp_str = resp.body_mut().read_to_string()
        .map_err(|e| format!("读取响应失败: {}", e))?;
    let resp_json: serde_json::Value = serde_json::from_str(&resp_str)
        .map_err(|e| format!("解析响应失败: {}", e))?;

    debug!("remote_stats: 查询完成");
    Ok(resp_json)
}
