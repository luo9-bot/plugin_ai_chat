//! 模型调用的唯一入口：重试与熔断
//!
//! 原先主回复路径是"一次裸 HTTP 调用，失败就沉默"：`CLAUDE.md` 声称
//! "默认 60s 超时，重试 3 次"，但代码里既没有重试也没有熔断。上游挂掉时
//! 每条消息仍然发一次请求、等一次完整超时——而消息处理是单队列串行的，
//! 于是"上游慢"直接变成"整只机器人慢"。
//!
//! 这里提供的语义（见 `.docs/counter-world-design.md` §4.3.2）：
//! - 可重试错误（超时、连接、5xx、429）按指数退避重试，次数有上限；
//! - 4xx 立即上抛：那是我们的请求写错了；
//! - 连续可重试失败达到阈值后**打开熔断**，打开期间立即失败，
//!   不再让每条消息都去等一次超时。

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use tracing::{debug, warn};

use super::error::LlmError;
use super::provider::no_error_agent;
use super::types::{ChatRequest, ChatResponse};
use crate::config;

/// 单次调用最多尝试几次（含首次）
const MAX_ATTEMPTS: u32 = 3;
/// 首次退避时长，之后按 2^n 增长
const BASE_BACKOFF_MS: u64 = 250;
/// 连续可重试失败达到这个数就打开熔断
const BREAKER_THRESHOLD: u32 = 5;
/// 熔断打开多久（秒）
const BREAKER_OPEN_SECS: u64 = 60;

/// 连续可重试失败计数
static CONSECUTIVE_FAILURES: AtomicU32 = AtomicU32::new(0);
/// 熔断打开到什么时候（unix 秒）；0 表示关闭
static BREAKER_OPEN_UNTIL: AtomicU64 = AtomicU64::new(0);

/// 熔断当前是否打开；打开时返回剩余秒数
fn breaker_remaining_secs(now: u64) -> Option<u64> {
    let until = BREAKER_OPEN_UNTIL.load(Ordering::Acquire);
    if until > now { Some(until - now) } else { None }
}

/// 调用成功：清空失败计数并关闭熔断
fn record_success() {
    CONSECUTIVE_FAILURES.store(0, Ordering::Release);
    BREAKER_OPEN_UNTIL.store(0, Ordering::Release);
}

/// 调用失败：累计可重试失败，达到阈值就打开熔断
fn record_failure(error: &LlmError, now: u64) {
    if !error.is_retryable() {
        // 请求本身有问题，与"上游不可用"无关，不该污染熔断计数
        return;
    }
    let failures = CONSECUTIVE_FAILURES.fetch_add(1, Ordering::AcqRel) + 1;
    if failures >= BREAKER_THRESHOLD {
        BREAKER_OPEN_UNTIL.store(now + BREAKER_OPEN_SECS, Ordering::Release);
        warn!(
            failures,
            open_secs = BREAKER_OPEN_SECS,
            "llm: 连续可重试失败达阈值，熔断打开"
        );
    }
}

/// 一次请求（不做重试）
fn attempt(req: &ChatRequest) -> Result<ChatResponse, LlmError> {
    let cfg = config::get();
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let json_body = serde_json::to_string(req).map_err(|e| LlmError::Serialize {
        source: e.to_string(),
    })?;

    let mut resp = no_error_agent()
        .post(&url)
        .header("Authorization", &format!("Bearer {}", cfg.api_key))
        .header("Content-Type", "application/json")
        .send(json_body.as_bytes())
        .map_err(|e| {
            // ureq 把超时/连接失败都归到 io 错误里，这里按是否 timeout 分类
            let text = e.to_string();
            let retryable = !text.contains("Invalid") && !text.contains("invalid url");
            LlmError::Transport {
                retryable,
                source: text,
            }
        })?;

    let status = resp.status().as_u16();
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| LlmError::Transport {
            retryable: true,
            source: e.to_string(),
        })?;

    if !(200..300).contains(&status) {
        return Err(LlmError::Status { code: status, body });
    }

    let parsed: ChatResponse = serde_json::from_str(&body).map_err(|e| LlmError::Malformed {
        source: e.to_string(),
    })?;
    if parsed.choices.is_empty() {
        return Err(LlmError::EmptyChoices);
    }
    Ok(parsed)
}

/// 带重试与熔断的模型调用
///
/// 这是**唯一**的调用入口：任何绕过它的直接 HTTP 调用都不会受熔断保护。
pub(crate) fn chat_completion(req: &ChatRequest) -> Result<ChatResponse, LlmError> {
    let now = crate::util::now_secs();
    if let Some(remaining) = breaker_remaining_secs(now) {
        debug!(remaining, "llm: 熔断打开，立即失败");
        return Err(LlmError::BreakerOpen {
            retry_after_secs: remaining,
        });
    }

    let mut last_error = None;
    for attempt_index in 0..MAX_ATTEMPTS {
        match attempt(req) {
            Ok(response) => {
                record_success();
                return Ok(response);
            }
            Err(error) => {
                let retryable = error.is_retryable();
                record_failure(&error, crate::util::now_secs());
                debug!(
                    attempt = attempt_index + 1,
                    kind = error.kind(),
                    retryable,
                    "llm: 调用失败"
                );
                if !retryable || attempt_index + 1 >= MAX_ATTEMPTS {
                    return Err(error);
                }
                last_error = Some(error);
                std::thread::sleep(backoff(attempt_index));
            }
        }
    }

    Err(last_error.unwrap_or(LlmError::EmptyChoices))
}

/// 指数退避 + 少量抖动，避免多个调用点同相位重试
fn backoff(attempt_index: u32) -> Duration {
    let base = BASE_BACKOFF_MS << attempt_index.min(6);
    let jitter = (fastrand::u64(0..=base / 4)).max(1);
    Duration::from_millis(base + jitter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_stays_bounded() {
        let first = backoff(0).as_millis();
        let second = backoff(1).as_millis();
        let third = backoff(2).as_millis();
        assert!(first >= BASE_BACKOFF_MS as u128);
        assert!(second > first, "退避必须增长：{first} -> {second}");
        assert!(third > second, "退避必须增长：{second} -> {third}");
        // 抖动有上限，不会失控
        let capped = backoff(30).as_millis();
        assert!(
            capped <= (BASE_BACKOFF_MS << 6) as u128 * 2,
            "退避不该无限增长：{capped}"
        );
    }

    /// 熔断是**全局**状态，因此整个状态机只在一个测试里走完。
    ///
    /// 拆成多个 `#[test]` 会让它们并行交错、互相覆盖 `BREAKER_OPEN_UNTIL`——
    /// 这是实际发生过的 flake（同一个测试单独跑通过、全量跑失败）。
    #[test]
    fn breaker_state_machine() {
        let now = crate::util::now_secs();
        record_success();
        assert!(breaker_remaining_secs(now).is_none());

        // 不可重试的错误不该打开熔断
        for _ in 0..BREAKER_THRESHOLD * 2 {
            record_failure(&LlmError::EmptyChoices, now);
        }
        assert!(
            breaker_remaining_secs(now).is_none(),
            "请求类错误不该触发熔断"
        );

        // 可重试错误累计到阈值即打开
        for _ in 0..BREAKER_THRESHOLD {
            record_failure(
                &LlmError::Status {
                    code: 503,
                    body: String::new(),
                },
                now,
            );
        }
        let remaining = breaker_remaining_secs(now).expect("熔断应该已打开");
        assert!(remaining <= BREAKER_OPEN_SECS && remaining > 0);

        // 打开期间：立即失败，不发起请求（因此这个测试不碰网络）
        let req = ChatRequest {
            model: "test".into(),
            messages: Vec::new(),
            frequency_penalty: 0.0,
            presence_penalty: 0.0,
            temperature: 0.0,
            top_p: 1.0,
            max_tokens: 1,
            tools: None,
            tool_choice: None,
            thinking: None,
        };
        // ChatResponse 没有实现 Debug，因此不能用 expect_err
        let error = match chat_completion(&req) {
            Err(error) => error,
            Ok(_) => panic!("熔断打开时必须立即失败（不能再去等一次超时）"),
        };
        assert_eq!(error.kind(), "breaker_open");
        assert!(!error.is_retryable());

        // 成功即关闭
        record_success();
        assert!(breaker_remaining_secs(now).is_none());
    }
}
