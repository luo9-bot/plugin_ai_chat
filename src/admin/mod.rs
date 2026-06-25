mod ui;
mod backup;
mod handlers;

use tiny_http::{Header, Method, Request, Response, Server};
use tracing::{info, warn};

use crate::config;

// ── 工具函数 ────────────────────────────────────────────────────

fn json_response(status: u16, body: serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    let body_str = body.to_string();
    Response::from_string(body_str)
        .with_status_code(status)
        .with_header(
            Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap(),
        )
        .with_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap())
        .with_header(
            Header::from_bytes("Access-Control-Allow-Headers", "Authorization, Content-Type")
                .unwrap(),
        )
        .with_header(
            Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
                .unwrap(),
        )
}

fn ok(body: serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(200, body)
}

fn err(status: u16, msg: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(status, serde_json::json!({"error": msg}))
}

fn read_body(request: &mut Request) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    request
        .as_reader()
        .read_to_end(&mut buf)
        .map_err(|e| format!("read body: {}", e))?;
    Ok(buf)
}

fn parse_json<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, String> {
    serde_json::from_slice(body).map_err(|e| format!("parse json: {}", e))
}

fn check_auth(request: &Request) -> bool {
    let token = &config::get().admin.token;
    if token.is_empty() {
        return true;
    }
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().to_string().to_lowercase() == "authorization")
        .and_then(|h| h.value.as_str().strip_prefix("Bearer "))
        .map(|t| t == token)
        .unwrap_or(false)
}

fn path_segments(path: &str) -> Vec<&str> {
    path.trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect()
}

fn format_timestamp(ts: u64) -> String {
    let secs = ts % 60;
    let mins = (ts / 60) % 60;
    let hours = (ts / 3600) % 24;
    // 简单用 unix timestamp 做文件名，不依赖 chrono
    format!("{}{:02}{:02}{:02}", ts / 86400, hours, mins, secs)
}

// ── 主路由 ─────────────────────────────────────────────────────

fn route(request: &mut Request) -> Response<std::io::Cursor<Vec<u8>>> {
    let method = request.method().clone();
    let url = request.url().to_string();

    // CORS 预检
    if method == Method::Options {
        return ok(serde_json::json!({"ok": true}));
    }

    // 静态页面
    if method == Method::Get && (url == "/" || url == "/index.html") {
        return Response::from_string(ui::HTML)
            .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
    }

    // 登录端点（不需要 auth）
    if method == Method::Post && url == "/api/login" {
        let body = match read_body(request) {
            Ok(b) => b,
            Err(e) => return err(400, &e),
        };
        let val: serde_json::Value = match parse_json(&body) {
            Ok(v) => v,
            Err(e) => return err(400, &e),
        };
        let token = val.get("token").and_then(|v| v.as_str()).unwrap_or("");
        if token == config::get().admin.token {
            return ok(serde_json::json!({"ok": true}));
        }
        return err(403, "invalid token");
    }

    // 静态图片不需要 auth（img 标签无法带 Authorization header）
    if method == Method::Get && url.starts_with("/api/sticker/image/") {
        let segs_local = path_segments(url.split('?').next().unwrap_or(&url));
        if let Some(hash) = segs_local.get(3).copied() {
            return handlers::handle_sticker_image(hash);
        }
        return err(400, "hash required");
    }

    // 认证检查
    if !check_auth(request) {
        return err(401, "unauthorized");
    }

    // 读取 body（如果有）
    let body = if matches!(method, Method::Post | Method::Put) {
        match read_body(request) {
            Ok(b) => b,
            Err(e) => return err(400, &e),
        }
    } else {
        Vec::new()
    };

    // 解析路径
    let path = url.split('?').next().unwrap_or(&url);
    let segs = path_segments(path);

    if segs.is_empty() || segs[0] != "api" {
        return err(404, "not found");
    }

    let api_segs = &segs[1..]; // 去掉 "api"

    match api_segs.first() {
        Some(&"self-thoughts") => handlers::handle_self_thoughts(&method, &api_segs[1..], &body),
        Some(&"memory") => handlers::handle_memory(&method, &api_segs[1..], &body),
        Some(&"working-memory") => handlers::handle_working_memory(&method, &api_segs[1..], &body),
        Some(&"backups") => handlers::handle_backups(&method, &api_segs[1..], &body),
        Some(&"emotion") => handlers::handle_emotion(&method, &api_segs[1..], &body),
        Some(&"mental-state") => handlers::handle_mental_state(&method, &api_segs[1..], &body),
        Some(&"blocklist") => handlers::handle_blocklist(&method, &api_segs[1..], &body),
        Some(&"proactive") => handlers::handle_proactive(&method, &api_segs[1..], &body),
        Some(&"archive") => {
            if method == Method::Get {
                handlers::handle_archive()
            } else {
                err(405, "method not allowed")
            }
        }
        Some(&"backups") => handlers::handle_backups(&method, &api_segs[1..], &body),
        Some(&"schedule") => handlers::handle_schedule(&method, &body),
        Some(&"analytics") => handlers::handle_analytics(),
        Some(&"sync") => handlers::handle_sync(&method, &api_segs[1..], &body),
        Some(&"anti-injection") => handlers::handle_anti_injection(&method, &api_segs[1..], &body),
        Some(&"conversations") => handlers::handle_conversations(&method, &api_segs[1..]),
        Some(&"config") => handlers::handle_config(&method, &api_segs[1..], &body),
        Some(&"quota") => handlers::handle_quota(&method, &api_segs[1..]),
        Some(&"sticker") => {
            match api_segs.get(1).copied() {
                Some(hash) if api_segs.get(2).copied() == Some("image") => {
                    handlers::handle_sticker_image(hash)
                }
                Some("image") => {
                    if let Some(hash) = api_segs.get(2).copied() {
                        handlers::handle_sticker_image(hash)
                    } else {
                        err(400, "hash required")
                    }
                }
                Some(hash) if api_segs.get(2).copied() == Some("tags") && method == Method::Put => {
                    handlers::handle_sticker_tags(hash, &body)
                }
                Some(hash) if api_segs.get(2).copied() == Some("description") && method == Method::Put => {
                    handlers::handle_sticker_description(hash, &body)
                }
                Some(hash) if method == Method::Post => handlers::handle_sticker_toggle(hash),
                Some(hash) if method == Method::Delete => handlers::handle_sticker_delete(hash),
                _ => handlers::handle_sticker(),
            }
        }
        Some(&"dashboard") => handlers::handle_dashboard(),
        Some(&"humanity") => handlers::handle_humanity(),
        Some(&"memory-ops-log") => handlers::handle_memory_ops_log(&method, &api_segs[1..]),
        Some(&"relationships") => handlers::handle_relationships(&method, &api_segs[1..]),
        Some(&"info") => handlers::handle_info(),
        Some(&"version") => ok(serde_json::json!({"version": env!("CARGO_PKG_VERSION")})),
        _ => err(404, "not found"),
    }
}

// ── 启动服务器 ──────────────────────────────────────────────────

pub fn start_server() {
    let cfg = &config::get().admin;
    let base_port = cfg.port;
    let mut port = base_port;
    let server = loop {
        let addr = format!("0.0.0.0:{}", port);
        match Server::http(&addr) {
            Ok(s) => break s,
            Err(e) => {
                if port < base_port + 5 {
                    warn!(port, error = %e, "admin: port busy, trying next");
                    port += 1;
                } else {
                    warn!(error = %e, "admin: failed to bind any port, giving up");
                    return;
                }
            }
        }
    };
    info!(port, "admin: server started at http://0.0.0.0:{}", port);

    for mut request in server.incoming_requests() {
        let response = route(&mut request);
        request.respond(response).ok();
    }
}
