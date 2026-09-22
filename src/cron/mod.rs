use luo9_sdk::Bot;

/// 处理定时任务触发事件
pub(crate) fn handle_task_event(json: &str) {
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
        let msg = crate::util::to_c_string(format!("[{}]\n{}", title, content));
        Bot::send_group_msg(group_id, msg);
    }
}
