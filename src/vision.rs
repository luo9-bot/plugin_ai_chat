use tracing::debug;
use crate::config;

/// 从消息中提取 [CQ:image,...] 的图片 URL
pub fn extract_image_urls(message: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut remaining = message;
    while let Some(start) = remaining.find("[CQ:image,") {
        let after = &remaining[start + 10..];
        if let Some(end) = after.find(']') {
            let cq_content = &after[..end];
            // 提取 url= 字段
            if let Some(url_start) = cq_content.find("url=") {
                let url_part = &cq_content[url_start + 4..];
                let url = if let Some(comma) = url_part.find(',') {
                    &url_part[..comma]
                } else {
                    url_part
                };
                if !url.is_empty() {
                    urls.push(url.to_string());
                }
            }
            remaining = &after[end + 1..];
        } else {
            break;
        }
    }
    urls
}

/// 去除消息中的 [CQ:image,...] 标签，返回纯文本
pub fn strip_image_cq(message: &str) -> String {
    let mut result = String::with_capacity(message.len());
    let mut remaining = message;
    while let Some(start) = remaining.find("[CQ:image,") {
        result.push_str(&remaining[..start]);
        let after = &remaining[start + 10..];
        if let Some(end) = after.find(']') {
            remaining = &after[end + 1..];
        } else {
            // 不完整的 CQ 码，保留剩余部分
            result.push_str(&remaining[start..]);
            return result.trim().to_string();
        }
    }
    result.push_str(remaining);
    result.trim().to_string()
}

/// 调用识图 API，返回图片描述
///
/// 使用 OpenAI responses API 格式：POST {base_url}/responses
/// 如果 api_key 未配置或调用失败，返回 None
pub fn recognize(image_url: &str) -> Option<String> {
    let cfg = config::get();
    if !cfg.vision.enabled() {
        return None;
    }

    let prompt = cfg.vision.prompt.clone();
    let model = cfg.vision.model.clone();
    let max_tokens = cfg.vision.max_tokens;

    let request_body = serde_json::json!({
        "model": model,
        "input": [{
            "role": "user",
            "content": [
                {
                    "type": "input_image",
                    "image_url": image_url
                },
                {
                    "type": "input_text",
                    "text": prompt
                }
            ]
        }],
        "max_output_tokens": max_tokens
    });

    let url = format!(
        "{}/responses",
        cfg.vision.base_url.trim_end_matches('/')
    );

    debug!(url = %url, model = %cfg.vision.model, image = %image_url, "vision: sending request");

    let json_body = match serde_json::to_string(&request_body) {
        Ok(j) => j,
        Err(e) => {
            debug!(error = %e, "vision: serialize failed");
            return None;
        }
    };

    let mut resp = match ureq::post(&url)
        .header("Authorization", &format!("Bearer {}", cfg.vision.api_key))
        .header("Content-Type", "application/json")
        .send(json_body.as_bytes())
    {
        Ok(r) => r,
        Err(e) => {
            debug!(error = %e, "vision: request failed");
            return None;
        }
    };

    let resp_str = match resp.body_mut().read_to_string() {
        Ok(s) => s,
        Err(e) => {
            debug!(error = %e, "vision: read response failed");
            return None;
        }
    };

    // 解析 responses API 格式
    // 标准格式: { "output": [{ "type": "message", "content": [{ "type": "output_text", "text": "..." }] }] }
    // 兼容 chat completions: { "choices": [{ "message": { "content": "..." } }] }
    let text = match serde_json::from_str::<serde_json::Value>(&resp_str) {
        Ok(v) => {
            // 尝试 responses 格式
            if let Some(output) = v.get("output").and_then(|o| o.as_array()) {
                output.iter().find_map(|item| {
                    item.get("content").and_then(|c| c.as_array()).and_then(|contents| {
                        contents.iter().find_map(|content| {
                            content.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                        })
                    })
                })
            }
            // 兼容 chat completions 格式
            else if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
                choices.first().and_then(|c| {
                    c.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()).map(|s| s.to_string())
                })
            }
            // 兼容直接 text 字段
            else {
                v.get("output_text").and_then(|t| t.as_str()).map(|s| s.to_string())
                    .or_else(|| v.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()))
            }
        }
        Err(e) => {
            debug!(error = %e, "vision: response parse failed");
            None
        }
    };

    match &text {
        Some(t) => {
            debug!(result = %t, "vision: got description");
            Some(t.clone())
        }
        None => {
            debug!(response = %resp_str, "vision: could not extract text from response");
            None
        }
    }
}
