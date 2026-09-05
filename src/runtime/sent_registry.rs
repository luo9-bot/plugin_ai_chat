//! 自己消息的发送回执登记
//!
//! 核心在发送成功后把 NapCat 返回的 message_id 发布到 `luo9_sent` 主题。
//! 本模块维护"她发出的内容 → message_id"的对应关系，
//! 这是撤回、引用回复、表情回应等表达行为的数据前提。

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 回执主题名（与核心/SDK 约定一致）
pub const TOPIC_SENT: &str = "luo9_sent";

/// 登记保留时长与容量上限
const RECORD_TTL: Duration = Duration::from_secs(24 * 3600);
const MAX_RECORDS: usize = 100;

/// 一条她自己发出的消息
pub struct SentRecord {
    pub group_id: Option<u64>,
    pub user_id: u64,
    pub message_id: u64,
    pub content: String,
    received: Instant,
}

static REGISTRY: Mutex<VecDeque<SentRecord>> = Mutex::new(VecDeque::new());

/// 核心回执 JSON（SDK `BusPayload::Sent` 外层标签格式）：
/// `{"Sent": {"group_id":..., "user_id":..., "message_id":..., "message":...}}`
///
/// 通过 SDK 的 `BusPayload::parse` 解析，`Sent` 以外的载荷返回 false。
pub fn record_from_json(json: &str) -> bool {
    let Some(luo9_sdk::payload::BusPayload::Sent(payload)) =
        luo9_sdk::payload::BusPayload::parse(json)
    else {
        return false;
    };
    if payload.message_id == 0 {
        return false;
    }
    record(
        payload.group_id,
        payload.user_id,
        payload.message_id,
        &payload.message,
    );
    true
}

/// 登记一条发送记录（自动清理过期与超量条目）
pub fn record(group_id: Option<u64>, user_id: u64, message_id: u64, content: &str) {
    if let Ok(mut registry) = REGISTRY.lock() {
        registry.retain(|r| r.received.elapsed() < RECORD_TTL);
        registry.push_back(SentRecord {
            group_id,
            user_id,
            message_id,
            content: content.to_string(),
            received: Instant::now(),
        });
        while registry.len() > MAX_RECORDS {
            registry.pop_front();
        }
    }
}

/// 该会话最近一条自己消息的 message_id
pub fn recent_message_id(group_id: Option<u64>, user_id: u64) -> Option<u64> {
    REGISTRY
        .lock()
        .ok()?
        .iter()
        .rev()
        .find(|r| r.group_id == group_id && r.user_id == user_id)
        .map(|r| r.message_id)
}

/// 按内容精确匹配查找 message_id（内容含 CQ 码原样比较）
pub fn find_by_content(group_id: Option<u64>, content: &str) -> Option<u64> {
    REGISTRY
        .lock()
        .ok()?
        .iter()
        .rev()
        .find(|r| r.group_id == group_id && r.content == content)
        .map(|r| r.message_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_finds_recent() {
        record(Some(100), 42, 1001, "你好呀");
        record(Some(100), 42, 1002, "在吗");
        assert_eq!(recent_message_id(Some(100), 42), Some(1002));
        assert_eq!(find_by_content(Some(100), "你好呀"), Some(1001));
        assert_eq!(find_by_content(Some(100), "不存在"), None);
    }

    #[test]
    fn parses_envelope_json() {
        assert!(record_from_json(
            r#"{"Sent":{"group_id":7,"user_id":8,"message_id":99,"message":"嗯"}}"#
        ));
        assert_eq!(find_by_content(Some(7), "嗯"), Some(99));
        assert!(!record_from_json("不是回执"));
        assert!(!record_from_json(r#"{"Sent":{"message_id":0}}"#));
    }
}
