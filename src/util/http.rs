//! 共享的 HTTP agent
//!
//! 两件事必须由这里统一，否则每个调用点都会各自犯错：
//!
//! 1. **超时必填**。ureq 的 `Timeouts::default()` 六个字段全是 `None`，
//!    即"永不超时"。消息处理是单队列串行的，一个被黑洞的连接会让所有群和
//!    所有私聊一起停摆，所以 `AgentSpec::timeout_secs` 没有默认值。
//! 2. **复用连接池**。`ureq::Agent` 持有连接池，每次调用新建一个等于每个请求
//!    都重新做 TCP + TLS 握手。这里按 spec 缓存 agent。

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

/// 连接建立的上限（秒）
///
/// 它包含在全局超时之内，单独设是因为"连不上"和"服务端不回"是两种故障：
/// 前者应该在几秒内上抛，而不是把整个全局预算耗在 TCP 握手上。
const CONNECT_TIMEOUT_SECS: u64 = 10;

/// 一个 agent 的全部行为差异
///
/// 用显式类型而不是两个布尔参数：调用点必须写明它要的是哪种语义。
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AgentSpec {
    /// 整个请求（含连接、发送、接收）的墙钟上限，必须大于 0
    pub timeout_secs: u64,
    /// 是否把 4xx/5xx 当作 ureq 错误
    ///
    /// `true` 适合"只看成功响应"的调用；`false` 让调用方能读到错误响应体，
    /// 用于排查上游为什么拒绝。
    pub status_as_error: bool,
}

impl AgentSpec {
    /// 不把 HTTP 错误状态码当作错误（4xx/5xx 的响应体可读）
    pub(crate) fn reading_error_body(timeout_secs: u64) -> Self {
        Self {
            timeout_secs,
            status_as_error: false,
        }
    }

    /// 把 HTTP 错误状态码当作错误
    pub(crate) fn requiring_success(timeout_secs: u64) -> Self {
        Self {
            timeout_secs,
            status_as_error: true,
        }
    }

    /// 实际生效的全局超时（秒）
    ///
    /// `ai.request_timeout` 配成 0 时按 1 秒处理：ureq 的 `None` 意思是
    /// "永不超时"，那是本模块存在的理由，不能被一个配置值退回去。
    fn effective_timeout_secs(self) -> u64 {
        self.timeout_secs.max(1)
    }

    /// 实际生效的连接超时（秒）：不超过全局超时
    fn effective_connect_timeout_secs(self) -> u64 {
        CONNECT_TIMEOUT_SECS.min(self.effective_timeout_secs())
    }

    fn build(self) -> ureq::Agent {
        let config = ureq::config::Config::builder()
            .http_status_as_error(self.status_as_error)
            .timeout_global(Some(Duration::from_secs(self.effective_timeout_secs())))
            .timeout_connect(Some(Duration::from_secs(
                self.effective_connect_timeout_secs(),
            )))
            .build();
        ureq::Agent::new_with_config(config)
    }
}

/// 按 spec 缓存的 agent 池
static AGENTS: LazyLock<Mutex<HashMap<AgentSpec, ureq::Agent>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 取一个满足 `spec` 的 agent（连接池跨调用复用）
///
/// 配置改了超时值会自然取到另一个缓存项，因此不需要额外的失效逻辑。
pub(crate) fn agent(spec: AgentSpec) -> ureq::Agent {
    let mut agents = AGENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    agents.entry(spec).or_insert_with(|| spec.build()).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_can_never_be_absent() {
        // "永不超时"必须不可表达：0 被抬到 1 秒
        assert_eq!(AgentSpec::reading_error_body(0).effective_timeout_secs(), 1);
        assert_eq!(
            AgentSpec::requiring_success(60).effective_timeout_secs(),
            60
        );
    }

    #[test]
    fn connect_timeout_is_capped_and_never_absent() {
        // 全局预算比连接上限短时，连接上限跟着缩短：否则"连不上"会吃掉整个预算
        assert_eq!(
            AgentSpec::reading_error_body(3).effective_connect_timeout_secs(),
            3
        );
        // 全局预算充足时用独立的连接上限
        assert_eq!(
            AgentSpec::reading_error_body(60).effective_connect_timeout_secs(),
            CONNECT_TIMEOUT_SECS
        );
        // 0 秒配置同样不能退化成"不设连接超时"
        assert_eq!(
            AgentSpec::reading_error_body(0).effective_connect_timeout_secs(),
            1
        );
    }
}
