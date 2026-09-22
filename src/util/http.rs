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

use super::sync::MutexExt;

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
    let mut agents = AGENTS.lock_recover();
    agents.entry(spec).or_insert_with(|| spec.build()).clone()
}

/// 图片等非 AI 端点的墙钟上限（秒）
///
/// 它不受 `ai.request_timeout` 管辖（那是模型端点的预算），但同样不能没有
/// 上限：下载发生在串行的消息处理队列上，卡住的是所有群和所有私聊。
pub(crate) const IMAGE_DOWNLOAD_TIMEOUT_SECS: u64 = 30;

/// 共享 HTTP 调用的失败原因
///
/// 分成两类是因为它们的处置方式不同：传输失败通常值得重试或告警，读不出
/// 响应体则说明对方答应了却没说完整。两者都装箱是因为 `ureq::Error` 体积
/// 不小，而这里每个调用点都会把它塞进 `Result`。
#[derive(Debug)]
pub(crate) enum HttpError {
    /// 连接、超时、TLS 等传输层失败
    Transport(Box<ureq::Error>),
    /// 响应体读取失败
    Body(Box<ureq::Error>),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(error) => write!(f, "传输失败：{error}"),
            Self::Body(error) => write!(f, "读取响应体失败：{error}"),
        }
    }
}

/// 用 `agent` 以 Bearer 认证 POST 一段 JSON，返回响应体
///
/// 四个 AI 端点（对话、识图、表情包选择、embedding）都要做同一件事。放在
/// 这里不只是省几行：手工拼请求时最容易漏掉的就是**超时**，而漏掉它意味着
/// 一个黑洞连接能让串行消息队列永久停摆。
///
/// 状态码不是错误（`AgentSpec::reading_error_body`）：上游拒绝时响应体里
/// 写着原因，调用方需要读到它才能知道为什么。
pub(crate) fn post_json(
    agent: &ureq::Agent,
    url: &str,
    api_key: &str,
    json_body: &str,
) -> Result<String, HttpError> {
    let mut response = agent
        .post(url)
        .header("Authorization", &format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .send(json_body.as_bytes())
        .map_err(|error| HttpError::Transport(Box::new(error)))?;

    response
        .body_mut()
        .read_to_string()
        .map_err(|error| HttpError::Body(Box::new(error)))
}

/// 下载一个 URL 的字节内容（图片等非 AI 端点）
///
/// 超时由本模块决定而不是调用方：这是一类固定的操作，调用方没有理由给出
/// 不同的上限，也没有机会忘记给。
pub(crate) fn get_bytes(url: &str) -> Result<Vec<u8>, HttpError> {
    agent(AgentSpec::requiring_success(IMAGE_DOWNLOAD_TIMEOUT_SECS))
        .get(url)
        .call()
        .map_err(|error| HttpError::Transport(Box::new(error)))?
        .into_body()
        .read_to_vec()
        .map_err(|error| HttpError::Body(Box::new(error)))
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

    /// 起一个只受理一次请求的本地服务器，返回基地址与接收线程
    fn one_shot_server<F>(handle_request: F) -> (String, std::thread::JoinHandle<()>)
    where
        F: FnOnce(&mut tiny_http::Request) -> String + Send + 'static,
    {
        let server = tiny_http::Server::http("127.0.0.1:0").expect("测试服务器应当能绑定本地端口");
        let addr = server
            .server_addr()
            .to_ip()
            .expect("应当是 IP 地址")
            .to_string();
        let join = std::thread::spawn(move || {
            let mut request = server.recv().expect("应当收到请求");
            let body = handle_request(&mut request);
            // 客户端可能已经超时断开，此时回写失败属于预期
            let _ = request.respond(tiny_http::Response::from_string(body));
        });
        (format!("http://{addr}"), join)
    }

    #[test]
    fn post_json_carries_the_bearer_header_and_returns_the_body() {
        let (base, join) = one_shot_server(|request| {
            let auth = request
                .headers()
                .iter()
                .find(|header| header.field.equiv("Authorization"))
                .map(|header| header.value.as_str().to_string())
                .unwrap_or_default();
            let mut sent = String::new();
            let _ = request.as_reader().read_to_string(&mut sent);
            format!("{auth}|{sent}")
        });

        let got = post_json(
            &agent(AgentSpec::reading_error_body(5)),
            &format!("{base}/responses"),
            "sk-test",
            "{\"a\":1}",
        );

        assert_eq!(got.expect("请求应当成功"), "Bearer sk-test|{\"a\":1}");
        join.join().ok();
    }

    /// 这个测试是整份模块存在的理由：服务器不回答时，调用方必须自己收场
    #[test]
    fn a_server_that_never_answers_cannot_hold_the_caller() {
        let (base, _detached) = one_shot_server(|_| {
            // 故意拖过客户端超时
            std::thread::sleep(Duration::from_secs(3));
            "too late".to_string()
        });

        let started = std::time::Instant::now();
        let result = post_json(
            &agent(AgentSpec::reading_error_body(1)),
            &format!("{base}/responses"),
            "sk-test",
            "{}",
        );
        let elapsed = started.elapsed();

        assert!(result.is_err(), "超时必须让调用失败，而不是一直等服务器");
        assert!(
            elapsed < Duration::from_secs(3),
            "应当在 1 秒超时附近返回，实际 {elapsed:?}"
        );
    }

    #[test]
    fn get_bytes_reports_transport_failure_instead_of_hanging() {
        // 127.0.0.1:1 上不会有服务，连接应当立刻被拒绝
        let error = get_bytes("http://127.0.0.1:1/nothing").expect_err("连接应当失败");
        assert!(
            matches!(error, HttpError::Transport(_)),
            "应当是传输层失败，实际 {error}"
        );
    }
}
