use tracing::{debug, info};

use crate::config;

use super::store::{SelfMemoryStore, SelfThought};

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
pub fn sync_to_remote(thought: &SelfThought) {
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
