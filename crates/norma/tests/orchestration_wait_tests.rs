use norma::cli_runtime::{classify_wait_status, classify_wait_task, WaitStatusDecision};
use norma::protocol::AgentTaskReceiptStatus;

/// CLI wait 状态机：终态立即返回，untracked 明确拒绝，运行态继续低频等待。
///
/// 数据构造（含关键数值的推导过程）：
///   终态数 = 4：done、failed、cancelled、interrupted
///   不可信等待态数 = 1：untracked
///   可继续等待态数 = 4：accepted、submitted、queued、cancel_requested
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将四个终态交给 wait 分类纯函数。
///   2. 将 untracked 交给同一分类边界。
///   3. 将四个非终态交给分类边界。
///
/// 预期结果：
///   - 正断言：四个终态均 Complete，untracked 为 Untracked。
///   - 正断言：四个非终态均 Continue。
///   - 负断言：untracked 不得进入超时轮询或伪装为完成。
#[test]
fn wait_classifies_terminal_untracked_and_running_states_truthfully() {
    assert_eq!(
        classify_wait_status(AgentTaskReceiptStatus::Done),
        WaitStatusDecision::Complete,
        "done 必须立即结束 wait"
    );
    assert_eq!(
        classify_wait_status(AgentTaskReceiptStatus::Failed),
        WaitStatusDecision::Complete,
        "failed 是可信终态，wait 必须立即返回真实状态"
    );
    assert_eq!(
        classify_wait_status(AgentTaskReceiptStatus::Cancelled),
        WaitStatusDecision::Complete,
        "cancelled 必须立即结束 wait"
    );
    assert_eq!(
        classify_wait_status(AgentTaskReceiptStatus::Interrupted),
        WaitStatusDecision::Complete,
        "interrupted 必须立即结束 wait"
    );
    assert_eq!(
        classify_wait_status(AgentTaskReceiptStatus::Untracked),
        WaitStatusDecision::Untracked,
        "untracked 无法可信确认终态，必须立即返回 task_untracked"
    );
    assert_eq!(
        classify_wait_status(AgentTaskReceiptStatus::Accepted),
        WaitStatusDecision::Continue,
        "accepted 尚未终结，wait 应继续"
    );
    assert_eq!(
        classify_wait_status(AgentTaskReceiptStatus::Submitted),
        WaitStatusDecision::Continue,
        "submitted 尚未终结，wait 应继续"
    );
    assert_eq!(
        classify_wait_status(AgentTaskReceiptStatus::Queued),
        WaitStatusDecision::Continue,
        "queued 尚未终结，wait 应继续"
    );
    assert_eq!(
        classify_wait_status(AgentTaskReceiptStatus::CancelRequested),
        WaitStatusDecision::Continue,
        "cancel_requested 在可信中断确认前仍应继续等待"
    );
    assert_ne!(
        classify_wait_status(AgentTaskReceiptStatus::Untracked),
        WaitStatusDecision::Complete,
        "untracked 不得伪造为可信完成"
    );
}

/// CLI wait 追踪边界：状态仍为 submitted 时，untracked 追踪也必须立即拒绝等待。
///
/// 数据构造（含关键数值的推导过程）：
///   wire status    = submitted（1 个非终态）
///   tracking modes = managed + untracked = 2 种
///   Total          = 1 × 2 = 2 个分类输入
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 submitted + managed 分类 → 允许继续轮询。
///   2. 用相同 submitted + untracked 分类 → 立即判定无法可信等待。
///
/// 预期结果：
///   - 正断言：managed submitted 返回 Continue。
///   - 正断言：untracked submitted 返回 Untracked。
///   - 负断言：untracked 不得因状态字段尚未投影为 Untracked 而进入轮询。
#[test]
fn wait_uses_tracking_field_even_when_status_is_still_submitted() {
    assert_eq!(
        classify_wait_task(AgentTaskReceiptStatus::Submitted, "managed"),
        WaitStatusDecision::Continue,
        "managed submitted 尚未终结，必须继续轮询"
    );
    assert_eq!(
        classify_wait_task(AgentTaskReceiptStatus::Submitted, "untracked"),
        WaitStatusDecision::Untracked,
        "untracked submitted 缺少可信 Hook，必须立即拒绝等待"
    );
    assert_ne!(
        classify_wait_task(AgentTaskReceiptStatus::Submitted, "untracked"),
        WaitStatusDecision::Continue,
        "untracked submitted 不得进入超时轮询"
    );
}

/// CLI wait 终态优先级：可信终态必须覆盖派发阶段遗留的 untracked 快照。
///
/// 数据构造（含关键数值的推导过程）：
///   terminal statuses = done + failed + cancelled + interrupted = 4 个
///   tracking modes    = untracked × 1
///   Total             = 4 × 1 = 4 个分类输入
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 依次输入四个已经由 Runtime/持久化确认的终态。
///   2. 每个终态都携带派发时遗留的 `tracking=untracked`。
///   3. wait 分类器先判断终态，再判断仍在运行的任务是否可追踪。
///
/// 预期结果：
///   - 正断言：四个终态都返回 Complete。
///   - 负断言：终态不得被降级为 Untracked，否则调用方无法取得真实结果。
#[test]
fn wait_returns_terminal_status_even_when_tracking_is_untracked() {
    assert_eq!(
        classify_wait_task(AgentTaskReceiptStatus::Done, "untracked"),
        WaitStatusDecision::Complete,
        "done 是可信终态，不得被遗留的 untracked tracking 覆盖"
    );
    assert_eq!(
        classify_wait_task(AgentTaskReceiptStatus::Failed, "untracked"),
        WaitStatusDecision::Complete,
        "failed 是可信终态，不得被遗留的 untracked tracking 覆盖"
    );
    assert_eq!(
        classify_wait_task(AgentTaskReceiptStatus::Cancelled, "untracked"),
        WaitStatusDecision::Complete,
        "cancelled 是可信终态，不得被遗留的 untracked tracking 覆盖"
    );
    assert_eq!(
        classify_wait_task(AgentTaskReceiptStatus::Interrupted, "untracked"),
        WaitStatusDecision::Complete,
        "interrupted 是可信终态，不得被遗留的 untracked tracking 覆盖"
    );
    assert_ne!(
        classify_wait_task(AgentTaskReceiptStatus::Done, "untracked"),
        WaitStatusDecision::Untracked,
        "已完成任务不得再次报告 task_untracked"
    );
}
