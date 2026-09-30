use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentInputSource,
    NormaToolRuntime, ToolCallPolicy, ToolContext,
};
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Adapter(AtomicUsize);
impl AgentCardAdapter for Adapter {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::Relaxed);
            match context.user_input.as_str() {
                "fail" => Err(AdapterError::Rejected("fixture rejection".into())),
                "unknown" => Ok(AdapterResult::unknown("fixture unknown outcome")),
                _ => Ok(AdapterResult::accepted("received, not completed").with_task_id(41)),
            }
        })
    }
}

fn context(input: &str) -> ToolContext {
    ToolContext {
        project_id: "project".into(),
        turn_id: "turn".into(),
        user_input: input.into(),
        input_source: AgentInputSource::Voice,
        contextual_delete_targets: vec![],
        tool_call_id: "function-call".into(),
        operation_id: None,
        cancellation: CancellationToken::new(),
    }
}

/// 共享入口预算：同一回合的克隆共享 20 次上限，下一回合有独立预算。
///
/// 数据构造：20 = 既有 Rig 单回合上限；第 21 = 20 + 1 次必须拒绝。
///   Adapter 返回 task_id=41 的 accepted，不能被升级为 completed。
/// 执行过程：1. 两个入口交替完成前 20 次调用。2. 第 21 次调用被挡在 Adapter 前。
///   3. 新回合执行一次，共 20 + 1 = 21 次真实 Adapter 调用。
/// 预期结果：前 20 次接收；溢出调用不存在；下一回合正常接收且不是任务完成。
#[tokio::test]
async fn shared_budget_stops_before_twenty_first_side_effect() {
    let adapter = Arc::new(Adapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let policy = ToolCallPolicy::default();
    let other = policy.clone();
    for index in 0..20 {
        let entry = if index % 2 == 0 { &policy } else { &other };
        entry
            .execute(&runtime, "create_terminal", json!({}), &context("execute"))
            .await
            .expect("前 20 次应能进入 Adapter");
    }
    let overflow = other
        .execute(&runtime, "create_terminal", json!({}), &context("execute"))
        .await;
    assert!(
        matches!(overflow, Err(AdapterError::Rejected(_))),
        "第 21 次必须拒绝，不能执行副作用"
    );
    assert_eq!(
        adapter.0.load(Ordering::Relaxed),
        20,
        "溢出调用不得到达 Adapter"
    );
    let next = ToolCallPolicy::default()
        .execute(&runtime, "create_terminal", json!({}), &context("execute"))
        .await
        .expect("新回合预算应独立");
    assert_eq!(next.status, "accepted", "接收回执不能改写成任务完成");
    assert_eq!(next.task_id, Some(41), "真实任务身份应保留");
    assert_eq!(adapter.0.load(Ordering::Relaxed), 21, "新回合只执行一次");
}

/// 连续失败：成功清零，连续三次失败后禁止继续执行。
///
/// 数据构造：先失败 2 次 < 既有阈值 3；成功后再失败 3 次 = 阈值。
///   真实调用数 = 2 + 1 + 3 = 6，阻断的第 7 次不能进入 Adapter。
/// 执行过程：1. 两次失败。2. accepted 清零。3. 三次失败。4. 再请求写操作。
/// 预期结果：第一次成功可执行；失败状态清零；最后写操作被拒绝且无第 7 次副作用。
#[tokio::test]
async fn consecutive_failures_reset_only_on_known_success() {
    let adapter = Arc::new(Adapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let policy = ToolCallPolicy::default();
    for _ in 0..2 {
        policy
            .execute(&runtime, "create_terminal", json!({}), &context("fail"))
            .await
            .expect_err("构造失败");
    }
    assert!(
        !policy.failure_budget_exhausted(),
        "两次失败不能提前耗尽三次预算"
    );
    policy
        .execute(&runtime, "create_terminal", json!({}), &context("execute"))
        .await
        .expect("第三次成功应执行并清零");
    assert!(!policy.failure_budget_exhausted(), "成功应清零连续失败");
    for _ in 0..3 {
        policy
            .execute(&runtime, "create_terminal", json!({}), &context("fail"))
            .await
            .expect_err("构造三次连续失败");
    }
    assert!(policy.failure_budget_exhausted(), "三次连续失败应耗尽预算");
    let blocked = policy
        .execute(&runtime, "create_terminal", json!({}), &context("execute"))
        .await;
    assert!(blocked.is_err(), "预算耗尽后不能再次写入");
    assert_eq!(
        adapter.0.load(Ordering::Relaxed),
        6,
        "第七次调用不得进入 Adapter"
    );
}

/// 未知结果：一旦无法确认副作用，同回合任何入口都停止后续执行。
///
/// 数据构造：第一次返回 unknown；第二次本可成功，实际调用数应为 1 + 0 = 1。
/// 执行过程：1. 执行并记录未知结果。2. 克隆入口尝试下一次写操作。
/// 预期结果：unknown 不作成功返回；停止标记共享；第二次副作用不存在。
#[tokio::test]
async fn unknown_outcome_stops_all_cloned_entries() {
    let adapter = Arc::new(Adapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let policy = ToolCallPolicy::default();
    let other = policy.clone();
    let unknown = policy
        .execute(&runtime, "create_terminal", json!({}), &context("unknown"))
        .await;
    assert!(unknown.is_err(), "unknown 不能作为成功工具结果返回");
    assert!(other.unknown_seen(), "停止标记必须在同回合共享");
    let blocked = other
        .execute(&runtime, "create_terminal", json!({}), &context("execute"))
        .await;
    assert!(blocked.is_err(), "未知结果后的写操作必须阻断");
    assert_eq!(adapter.0.load(Ordering::Relaxed), 1, "不得出现第二次副作用");
}
