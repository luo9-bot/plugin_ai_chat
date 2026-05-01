use luo9_sdk::bus::Bus;
use luo9_sdk::Bot;
use serde_json::json;
use std::ffi::CString;

/// 处理 AI 回复中的定时任务请求
///
/// AI 回复格式:
/// {"cron":{"title":"...", "exp":"秒 分 时 日 月 星期 年", "content":"..."}}|cron|回复内容
///
/// 返回去除定时部分后的纯回复文本
pub fn handle_cron_in_reply(reply: &str, group_id: u64) -> String {
    let Some(pos) = reply.find("|cron|") else {
        return reply.to_string();
    };

    let cron_json_str = reply[..pos].trim();
    let normal_reply = reply[pos + 6..].trim();

    // 尝试解析定时请求
    match parse_cron_request(cron_json_str) {
        Ok((title, cron_exp, content)) => {
            // 注册定时任务
            let task_name = format!("ai_chat_cron_{}_{}", group_id, title);
            let req = json!({
                "action": "schedule",
                "task_name": task_name,
                "cron": cron_exp,
                "payload": json!({
                    "group_id": group_id,
                    "content": content,
                    "title": title
                }).to_string()
            });
            match Bus::topic("luo9_task_miso").publish(&req.to_string()) {
                Ok(_) => println!("[ai_chat] cron task registered: {} [{}]", title, cron_exp),
                Err(e) => eprintln!("[ai_chat] failed to register cron task: {:?}", e),
            }
        }
        Err(e) => {
            eprintln!("[ai_chat] failed to parse cron request: {}", e);
        }
    }

    normal_reply.to_string()
}

fn parse_cron_request(json_str: &str) -> Result<(String, String, String), String> {
    let v: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("invalid json: {}", e))?;

    let cron = v.get("cron").ok_or("missing 'cron' field")?;
    let title = cron.get("title").and_then(|v| v.as_str()).ok_or("missing 'title'")?;
    let exp = cron.get("exp").and_then(|v| v.as_str()).ok_or("missing 'exp'")?;
    let content = cron.get("content").and_then(|v| v.as_str()).ok_or("missing 'content'")?;

    Ok((title.to_string(), exp.to_string(), content.to_string()))
}

/// 处理定时任务触发事件
pub fn handle_task_event(json: &str) {
    let Ok(event) = serde_json::from_str::<serde_json::Value>(json) else {
        return;
    };
    let payload = event["payload"].as_str().unwrap_or("");

    // 尝试解析 payload
    let Ok(data) = serde_json::from_str::<serde_json::Value>(payload) else {
        return;
    };

    let group_id = data["group_id"].as_u64().unwrap_or(0);
    let content = data["content"].as_str().unwrap_or("");
    let title = data["title"].as_str().unwrap_or("提醒");

    if group_id > 0 && !content.is_empty() {
        let msg = CString::new(format!("[{}]\n{}", title, content)).unwrap();
        Bot::send_group_msg(group_id, msg);
    }
}
