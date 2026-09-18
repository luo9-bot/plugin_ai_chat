use crate::anti_injection;
use crate::config;
use tracing::{debug, info};

/// 从消息中提取 [CQ:image,...] 的图片 URL
pub(crate) fn extract_image_urls(message: &str) -> Vec<String> {
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
///
/// 只处理图片：视频/文件/转发等其它 CQ 码由 `conversation::turn::strip_cq_codes`
/// 统一剥离（那里有完整的 CQ 语法处理）。这里的语义就是"去掉图片码"，
/// 保留它是因为记忆与图片沉淀路径需要精确区分"这条消息本来有没有图"。
pub(crate) fn strip_image_cq(message: &str) -> String {
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
/// 注意：此函数需要 user_id 参数来检查识图禁用状态
pub(crate) fn recognize_for_user(image_url: &str, user_id: u64) -> Option<String> {
    // 检查用户是否被禁用识图
    if anti_injection::is_vision_disabled(user_id) {
        info!(user_id, "vision: 用户识图已被禁用");
        return None;
    }
    recognize(image_url)
}

/// VLM 调用的失败原因
///
/// 分成三类是因为处置方式不同：序列化失败是本地的编码 bug，传输失败通常是
/// 上游不可达，没有文本则说明对方答应了却没给出可用内容。
#[derive(Debug)]
pub(crate) enum VlmError {
    /// 请求体无法序列化
    Serialize(serde_json::Error),
    /// 传输失败或响应体读取失败
    Http(crate::util::HttpError),
    /// 响应里没有可提取的文本
    NoText,
}

impl std::fmt::Display for VlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialize(error) => write!(f, "请求体序列化失败：{error}"),
            Self::Http(error) => write!(f, "{error}"),
            Self::NoText => write!(f, "响应里没有可提取的文本"),
        }
    }
}

/// `{base_url}/responses`：识图与表情包选择共用的 VLM 端点
///
/// 这是**唯一**一处 VLM 调用与响应解析。此前识图和表情包选择各写了一份，
/// 连解析回退顺序都不一致——改一处必漏另一处。
///
/// 超时走 [`crate::ai::no_error_agent`]：VLM 也在串行消息队列的路径上，
/// "不设超时"等于给整个 bot 留一个无限期停摆的入口。
pub(crate) fn call_vlm(
    base_url: &str,
    api_key: &str,
    body: &serde_json::Value,
) -> Result<String, VlmError> {
    let url = format!("{}/responses", base_url.trim_end_matches('/'));
    let json_body = serde_json::to_string(body).map_err(VlmError::Serialize)?;

    debug!(url = %url, "vision: sending request");

    let response_body =
        crate::util::post_json(&crate::ai::no_error_agent(), &url, api_key, &json_body)
            .map_err(VlmError::Http)?;

    extract_vlm_text(&response_body).ok_or_else(|| {
        debug!(response = %response_body, "vision: 响应里没有可提取的文本");
        VlmError::NoText
    })
}

/// 从 VLM 响应里提取文本
///
/// 兼容三种上游形态，按可靠性从高到低回退：
/// 1. responses API：`{ "output": [{ "content": [{ "text": "..." }] }] }`
/// 2. chat completions：`{ "choices": [{ "message": { "content": "..." } }] }`
/// 3. 直接字段：`output_text` / `text`
fn extract_vlm_text(response_body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(response_body).ok()?;

    let from_responses = value
        .get("output")
        .and_then(|output| output.as_array())
        .and_then(|output| {
            output.iter().find_map(|item| {
                item.get("content")
                    .and_then(|content| content.as_array())
                    .and_then(|contents| {
                        contents.iter().find_map(|content| {
                            content
                                .get("text")
                                .and_then(|text| text.as_str())
                                .map(str::to_string)
                        })
                    })
            })
        });

    let from_choices = value
        .get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .map(str::to_string);

    let from_plain_text = ["output_text", "text"]
        .iter()
        .find_map(|key| value.get(key).and_then(|text| text.as_str()))
        .map(str::to_string);

    from_responses.or(from_choices).or(from_plain_text)
}

/// 调用识图 API，返回图片描述 (不检查用户禁用状态)
///
/// 使用 OpenAI responses API 格式：POST {base_url}/responses
/// 如果 api_key 未配置或调用失败，返回 None
pub(crate) fn recognize(image_url: &str) -> Option<String> {
    info!(url = %image_url, "vision: 开始识别图片");
    let cfg = config::get();
    if !cfg.vision.enabled() {
        return None;
    }

    let request_body = serde_json::json!({
        "model": cfg.vision.model,
        "input": [{
            "role": "user",
            "content": [
                {
                    "type": "input_image",
                    "image_url": image_url
                },
                {
                    "type": "input_text",
                    "text": crate::prompt::PromptManager::get().raw("vision_describe")
                }
            ]
        }],
        "max_output_tokens": cfg.vision.max_tokens
    });

    match call_vlm(&cfg.vision.base_url, &cfg.vision.api_key, &request_body) {
        Ok(text) => {
            debug!(result = %text, "vision: got description");
            Some(text)
        }
        Err(error) => {
            debug!(error = %error, "vision: 识图失败");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_api_shape_is_understood() {
        let body = r#"{"output":[{"content":[{"type":"output_text","text":"一只橘猫"}]}]}"#;
        assert_eq!(extract_vlm_text(body).as_deref(), Some("一只橘猫"));
    }

    #[test]
    fn chat_completions_shape_is_understood() {
        let body = r#"{"choices":[{"message":{"content":"一只橘猫"}}]}"#;
        assert_eq!(extract_vlm_text(body).as_deref(), Some("一只橘猫"));
    }

    #[test]
    fn plain_text_fields_are_understood() {
        assert_eq!(
            extract_vlm_text(r#"{"output_text":"甲"}"#).as_deref(),
            Some("甲")
        );
        assert_eq!(extract_vlm_text(r#"{"text":"乙"}"#).as_deref(), Some("乙"));
    }

    /// 有 output 键但里面没有文本时，必须继续往后面的形态回退
    #[test]
    fn an_empty_output_still_falls_back_to_choices() {
        let body = r#"{"output":[],"choices":[{"message":{"content":"回退成功"}}]}"#;
        assert_eq!(extract_vlm_text(body).as_deref(), Some("回退成功"));
    }

    #[test]
    fn junk_responses_yield_nothing_instead_of_panicking() {
        for body in ["", "not json at all", "{}", r#"{"output":"不是数组"}"#] {
            assert_eq!(extract_vlm_text(body), None, "{body:?}");
        }
    }

    #[test]
    fn image_urls_are_extracted_and_stripped() {
        let message = "看这个[CQ:image,file=a.jpg,url=https://example.com/a.jpg]好看吗";
        assert_eq!(
            extract_image_urls(message),
            vec!["https://example.com/a.jpg".to_string()]
        );
        assert_eq!(strip_image_cq(message), "看这个好看吗");
    }
}
