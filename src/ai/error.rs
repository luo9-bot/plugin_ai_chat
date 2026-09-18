//! 模型交互的错误与产出类型
//!
//! 这一层存在的理由：原先错误类型是 `String`，于是"HTTP 500"、"超时"、
//! "响应体形状不对"、"我们自己的请求序列化失败"在类型上**不可区分**；
//! 而"她选择沉默"与"API 全挂了"都表现为 `Ok(None)`，在日志里长得一模一样。
//!
//! 这里把两件事都变成穷尽枚举：调用方不可能"忘记处理某一类"。

/// 一次模型调用的失败原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmError {
    /// 请求体无法序列化——我们的 bug，重试没有意义
    Serialize { source: String },
    /// 传输层失败（连接、超时、读取）
    Transport { retryable: bool, source: String },
    /// 上游返回非 2xx
    Status { code: u16, body: String },
    /// 响应体不是我们认识的形状
    Malformed { source: String },
    /// 响应体合法但没有 choices
    EmptyChoices,
    /// 熔断打开：连续可重试失败过多，这段时间内立即失败，不占用等待
    BreakerOpen { retry_after_secs: u64 },
}

impl LlmError {
    /// 是否值得重试
    ///
    /// 4xx 不重试：那是"我们的请求写错了"，重试只会重复犯错。
    /// 熔断打开时不重试：它本身就是"别再打了"的信号。
    pub fn is_retryable(&self) -> bool {
        match self {
            LlmError::Transport { retryable, .. } => *retryable,
            // 5xx 与 429 是上游的临时状态
            LlmError::Status { code, .. } => *code >= 500 || *code == 429,
            LlmError::Serialize { .. }
            | LlmError::Malformed { .. }
            | LlmError::EmptyChoices
            | LlmError::BreakerOpen { .. } => false,
        }
    }

    /// 稳定的短标签，用于日志与统计（不要用 Debug 输出当标签）
    pub fn kind(&self) -> &'static str {
        match self {
            LlmError::Serialize { .. } => "serialize",
            LlmError::Transport { .. } => "transport",
            LlmError::Status { .. } => "status",
            LlmError::Malformed { .. } => "malformed",
            LlmError::EmptyChoices => "empty_choices",
            LlmError::BreakerOpen { .. } => "breaker_open",
        }
    }
}

impl std::fmt::Display for LlmError {
    /// 面向日志与既有 `Result<_, String>` 边界的描述
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::Serialize { source } => write!(f, "请求序列化失败: {source}"),
            LlmError::Transport { retryable, source } => {
                write!(f, "传输失败(retryable={retryable}): {source}")
            }
            LlmError::Status { code, body } => write!(f, "上游返回 {code}: {body}"),
            LlmError::Malformed { source } => write!(f, "响应体无法解析: {source}"),
            LlmError::EmptyChoices => write!(f, "响应没有 choices"),
            LlmError::BreakerOpen { retry_after_secs } => {
                write!(f, "熔断打开，{retry_after_secs}s 后重试")
            }
        }
    }
}

/// 她保持沉默的原因
///
/// **只有 [`SilenceCause::Chose`] 算"她不想说话"。** 其余三类是故障，
/// 必须留痕、可告警——把它们混为一谈会让"API 全挂了"看起来像"她今天很安静"。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SilenceCause {
    /// 她选择不说：合法结果
    Chose,
    /// 输出无效：把工具调用写成了文字，纠正若干次仍不合法
    InvalidOutput { attempts: u32 },
    /// 上游故障，这一轮无从表达
    UpstreamFailed { error: LlmError },
    /// 轮次预算用尽（模型一直在调用工具，没有给出表态）
    BudgetExhausted { rounds: u32 },
}

impl SilenceCause {
    /// 这是"她的决定"吗？（决定沉默 ≠ 没能说话）
    pub fn is_deliberate(&self) -> bool {
        matches!(self, SilenceCause::Chose)
    }

    pub fn kind(&self) -> &'static str {
        match self {
            SilenceCause::Chose => "chose",
            SilenceCause::InvalidOutput { .. } => "invalid_output",
            SilenceCause::UpstreamFailed { .. } => "upstream_failed",
            SilenceCause::BudgetExhausted { .. } => "budget_exhausted",
        }
    }
}

/// 这一轮她的产出
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Utterance {
    /// 她要说的内容
    Say(String),
    /// 她这一轮没有外发内容，以及为什么
    Silent(SilenceCause),
}

impl Utterance {
    /// 沉默原因（有内容时为 `None`）
    ///
    /// 调用方用模式匹配取要外发的文本，因此这里只暴露"为什么沉默"。
    pub fn silence_cause(&self) -> Option<&SilenceCause> {
        match self {
            Utterance::Say(_) => None,
            Utterance::Silent(cause) => Some(cause),
        }
    }

    /// 便捷构造：沉默
    pub fn silent(cause: SilenceCause) -> Self {
        Utterance::Silent(cause)
    }

    /// 便捷构造：上游故障导致的沉默
    pub fn failed(error: LlmError) -> Self {
        Utterance::Silent(SilenceCause::UpstreamFailed { error })
    }
}

/// 模型这一轮输出的形态（影子观测用）
///
/// 记录它只有一个目的：回答"如果强制 `Turn` tagged union，现在有多少轮
/// 会变成 schema 失败"。判断依据完全来自 `tool_loop` 已有的分支，
/// 因此观测本身不改变任何行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnShape {
    /// 裸文本 = 发言。**当前契约下是正常路径**，但在严格 Turn 下
    /// "发言"必须走 tool call，因此它会被判为无效输出
    PlainText,
    /// 空响应（她选择不说）。严格 Turn 下应当是一个 `finish` 调用
    EmptyResponse,
    /// 把 `finish` 调用写成了文字
    LeakedFinishText,
    /// 把其它工具调用写成了文字
    LeakedToolText,
    /// 一次干净的工具调用（不带旁白）
    ToolCallClean,
    /// 工具调用 + 同批旁白文本
    ToolCallWithNarration,
    /// 一次响应里含多个工具调用
    ToolCallMultiple,
    /// 轮次用尽（模型一直在调工具而没表态）
    RoundsExhausted,
    /// 上游故障（不是模型输出的问题，不参与契约合法率）
    UpstreamFailed,
}

impl TurnShape {
    pub fn as_str(self) -> &'static str {
        match self {
            TurnShape::PlainText => "plain_text",
            TurnShape::EmptyResponse => "empty_response",
            TurnShape::LeakedFinishText => "leaked_finish_text",
            TurnShape::LeakedToolText => "leaked_tool_text",
            TurnShape::ToolCallClean => "tool_call_clean",
            TurnShape::ToolCallWithNarration => "tool_call_with_narration",
            TurnShape::ToolCallMultiple => "tool_call_multiple",
            TurnShape::RoundsExhausted => "rounds_exhausted",
            TurnShape::UpstreamFailed => "upstream_failed",
        }
    }

    /// 在严格 Turn 契约（`tool_choice: required` + `deny_unknown_fields`）
    /// 下，这一形态是否是合法输出
    ///
    /// 只有"一次干净的工具调用"合法。上游故障不该算进契约合法率——
    /// 那是传输问题，不是模型没遵守 schema。
    pub fn is_valid_under_turn_contract(self) -> bool {
        matches!(self, TurnShape::ToolCallClean)
    }

    /// 是否参与契约合法率的统计
    pub fn counts_toward_contract(self) -> bool {
        !matches!(self, TurnShape::UpstreamFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_failure_stays_out_of_the_contract_ratio() {
        assert!(!TurnShape::UpstreamFailed.counts_toward_contract());
        for shape in [
            TurnShape::PlainText,
            TurnShape::EmptyResponse,
            TurnShape::LeakedFinishText,
            TurnShape::LeakedToolText,
            TurnShape::ToolCallClean,
            TurnShape::ToolCallWithNarration,
            TurnShape::ToolCallMultiple,
            TurnShape::RoundsExhausted,
        ] {
            assert!(shape.counts_toward_contract(), "{shape:?} 应参与统计");
        }
    }

    /// 只有"一次干净的工具调用"在严格 Turn 下合法——这正是观测要量化的代价
    #[test]
    fn only_a_clean_tool_call_satisfies_the_strict_contract() {
        assert!(TurnShape::ToolCallClean.is_valid_under_turn_contract());
        for shape in [
            TurnShape::PlainText,
            TurnShape::EmptyResponse,
            TurnShape::LeakedFinishText,
            TurnShape::LeakedToolText,
            TurnShape::ToolCallWithNarration,
            TurnShape::ToolCallMultiple,
            TurnShape::RoundsExhausted,
        ] {
            assert!(
                !shape.is_valid_under_turn_contract(),
                "{shape:?} 在严格契约下不应算合法"
            );
        }
    }

    #[test]
    fn shape_labels_are_stable() {
        // 标签会进数据库并被后台读取，改名等于破坏历史数据
        assert_eq!(TurnShape::PlainText.as_str(), "plain_text");
        assert_eq!(TurnShape::ToolCallClean.as_str(), "tool_call_clean");
        assert_eq!(TurnShape::RoundsExhausted.as_str(), "rounds_exhausted");
    }

    #[test]
    fn client_errors_are_not_retryable() {
        // 我们自己的请求写错了/形状不对：重试只会重复犯错
        assert!(!LlmError::Serialize { source: "x".into() }.is_retryable());
        assert!(!LlmError::Malformed { source: "x".into() }.is_retryable());
        assert!(!LlmError::EmptyChoices.is_retryable());
        assert!(
            !LlmError::BreakerOpen {
                retry_after_secs: 30
            }
            .is_retryable()
        );
        // 4xx：请求有问题
        assert!(
            !LlmError::Status {
                code: 400,
                body: String::new()
            }
            .is_retryable()
        );
    }

    #[test]
    fn upstream_transient_errors_are_retryable() {
        assert!(
            LlmError::Transport {
                retryable: true,
                source: "timeout".into()
            }
            .is_retryable()
        );
        assert!(
            !LlmError::Transport {
                retryable: false,
                source: "dns".into()
            }
            .is_retryable()
        );
        for code in [500, 502, 503, 504, 429] {
            assert!(
                LlmError::Status {
                    code,
                    body: String::new()
                }
                .is_retryable(),
                "{code} 应该是可重试的"
            );
        }
    }

    /// 只有"她选择沉默"是她的决定；其余三类都是故障
    #[test]
    fn only_a_choice_counts_as_her_being_quiet() {
        assert!(SilenceCause::Chose.is_deliberate());
        assert!(!SilenceCause::InvalidOutput { attempts: 2 }.is_deliberate());
        assert!(
            !SilenceCause::UpstreamFailed {
                error: LlmError::EmptyChoices
            }
            .is_deliberate()
        );
        assert!(!SilenceCause::BudgetExhausted { rounds: 3 }.is_deliberate());
    }

    #[test]
    fn utterance_separates_content_from_cause() {
        // 有内容时没有沉默原因；沉默时内容不可达——两者不相交
        let said = Utterance::Say("在的".into());
        assert!(said.silence_cause().is_none());
        match said {
            Utterance::Say(text) => assert_eq!(text, "在的"),
            Utterance::Silent(_) => panic!("Say 必须携带文本"),
        }

        let quiet = Utterance::silent(SilenceCause::Chose);
        assert_eq!(quiet.silence_cause(), Some(&SilenceCause::Chose));
    }

    #[test]
    fn upstream_failure_keeps_its_cause_in_the_value() {
        // 失败原因放在值里而不是 Err：调用方不可能"忽略错误"而丢掉它
        let failed = Utterance::failed(LlmError::Status {
            code: 503,
            body: "upstream".into(),
        });
        match failed.silence_cause() {
            Some(SilenceCause::UpstreamFailed { error }) => {
                assert_eq!(error.kind(), "status");
                assert!(error.is_retryable());
            }
            other => panic!("失败原因必须保留在值里，实际 {other:?}"),
        }
    }
}
