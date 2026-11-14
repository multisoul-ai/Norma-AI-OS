use brain::acp::{
    client::{AcpSession, SessionOptions},
    gate::Gate,
    trace::TraceKind,
};
use brain::BackendKind;
use norma::protocol::BrainScope;
use std::{sync::atomic::AtomicBool, time::Duration};

/// 场景：OpenCode 把格式拒绝转成 cancelled，宿主必须给具体纠正说明并在同会话续接。
/// 数据构造：stdio 适配器先发 phase && propose，再要求收到格式反馈才能发两条单独命令。
/// 执行：通过公开 AcpSession 和真实 Gate 发一轮；预期：组合被拒绝、单条重试放行，最终回复无用户拒绝误报。
#[test]
fn compound_task_commands_are_retried_with_specific_feedback() {
    let (reply, events, prompts) = run("recover", false);
    assert_eq!(
        reply.expect("host must recover format cancellation"),
        "RECOVERED",
        "最终回复不能保留错误的用户拒绝说法"
    );
    assert_eq!(prompts, 2, "只补发一次纠正，不重放原始任务");
    assert!(
        !events
            .iter()
            .any(|item| item.kind == TraceKind::ToolResult && item.text.contains("user rejected")),
        "已知格式错误不能显示为用户拒绝"
    );
}

/// 场景：组合前已成功执行 A，后续错误 A&&B 不能让 A 重放。
/// 数据构造：适配器先请求 A，再请求组合；执行：同会话纠正；预期：续接中只有 B。
#[test]
fn accepted_command_before_rejection_is_not_replayed() {
    let (reply, _, prompts) = run("prior-success", false);
    assert_eq!(
        reply.expect("recover remaining operation"),
        "RECOVERED",
        "已接受的命令不能进入未执行列表"
    );
    assert_eq!(prompts, 2, "只纠正剩余操作");
}

fn run(
    case: &str,
    user_cancel: bool,
) -> (
    Result<String, brain::acp::client::AcpError>,
    Vec<brain::acp::trace::TraceItem>,
    usize,
) {
    let root = tempfile::tempdir().expect("isolated retry");
    let log = root.path().join("prompts.jsonl");
    let options = SessionOptions {
        kind: BackendKind::OpenCode,
        program: "python3".into(),
        args: vec![format!(
            "{}/tests/support/fake_command_retry.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        cwd: root.path().into(),
        env: vec![
            ("RETRY_LOG".into(), log.display().to_string()),
            ("RETRY_CASE".into(), case.into()),
        ],
        meta: None,
        mode: None,
        model: None,
        effort: None,
        handshake_timeout: Duration::from_secs(5),
    };
    let cancel = AtomicBool::new(false);
    let mut session = AcpSession::start_staff_cancellable(&options, &cancel).expect("start");
    let gate = Gate::for_project("/opt/norma".into(), root.path().into()).for_task(
        BrainScope {
            workspace_id: "w".into(),
            route_id: "r".into(),
        },
        "t".into(),
    );

    let mut events = Vec::new();
    let timeout = if matches!(case, "timeout" | "timeout-feedback" | "late-end") {
        Duration::from_millis(200)
    } else {
        Duration::from_secs(5)
    };
    let reply = session.prompt("Delegate review", &gate, &cancel, timeout, &mut |item| {
        if user_cancel && item.kind == TraceKind::Permission {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if item.kind == TraceKind::Permission && case == "late-end" {
            std::thread::sleep(Duration::from_millis(80));
        }
        if item.kind == TraceKind::Prompt && case == "cancel-feedback" {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if item.kind == TraceKind::Prompt && case == "timeout-feedback" {
            std::thread::sleep(Duration::from_millis(220));
        }
        events.push(item);
    });
    let prompts = std::fs::read_to_string(log)
        .expect("prompts")
        .lines()
        .count();
    (reply, events, prompts)
}

/// 场景：格式错误后的部分成功及模型自主纠正不能触发重复操作。
/// 数据构造：partial 先接受 A，natural 接受 A/B；执行：等待 ACP 原轮结束。
/// 预期：partial 只续接 B；natural 不新增 prompt。
#[test]
fn partial_and_natural_recovery_do_not_duplicate_accepted_commands() {
    for (case, expected) in [("partial", 2), ("natural", 1)] {
        let (reply, _, prompts) = run(case, false);
        assert_eq!(reply.expect(case), "RECOVERED", "只完成未接受操作：{case}");
        assert_eq!(prompts, expected, "自然纠正后不能多开一轮：{case}");
    }
}

/// 场景：越权、用户取消、超时、持续错误都不能无限自动续接。
/// 数据构造：安全组合混入 true、cancel 标记、超过截止时间、每轮重复组合四种适配器。
/// 执行：同一公开 prompt；预期：前三种最多一轮，持续格式错误最多原轮+两次纠正。
#[test]
fn corrections_respect_policy_cancellation_timeout_and_retry_limit() {
    for (case, cancel, expected) in [
        ("unsafe", false, 1),
        ("recover", true, 1),
        ("timeout", false, 1),
        ("repeat", false, 3),
    ] {
        let (reply, events, prompts) = run(case, cancel);
        assert!(reply.is_err(), "边界结束必须可见：{case}");
        assert_eq!(prompts, expected, "不可额外重试：{case}");
        if case == "unsafe" {
            assert!(
                !events.iter().any(|item| item
                    .tool
                    .as_ref()
                    .is_some_and(|tool| tool.command_format_error)),
                "越权组合不能标成可纠正的格式错误"
            );
        }
    }
}

/// 场景：纠正说明回调期间取消或到期，不能先发 prompt 再在下一循环退出。
/// 数据构造：收到纠正 Prompt 事件时设置 cancel 或越过 200 ms 总期限。
/// 执行：公开 ACP prompt；预期：适配器只收到原始一轮，错误类型保留取消/超时。
#[test]
fn correction_send_rechecks_cancellation_and_deadline() {
    for case in ["cancel-feedback", "timeout-feedback"] {
        let (reply, _, prompts) = run(case, false);
        assert!(reply.is_err(), "取消或到期必须终止：{case}");
        assert_eq!(prompts, 1, "回调之后不能发送迟到续接：{case}");
    }
}

/// 场景：原轮响应在 recv 的 100 ms 等待期间越过 200 ms 总期限。
/// 数据构造：权限审计耗时 80 ms，适配器收到拒绝后 150 ms 才结束，即约 230 ms。
/// 执行：等待真实 stdio 响应；预期：不发布任何纠正 Prompt，终态为 timeout。
#[test]
fn late_turn_end_does_not_start_correction() {
    let (reply, events, _) = run("late-end", false);
    assert!(
        matches!(reply, Err(brain::acp::client::AcpError::Timeout)),
        "过期响应应报告超时"
    );
    assert!(
        !events.iter().any(|item| item.kind == TraceKind::Prompt),
        "deadline 之后不能发出纠正说明或续接"
    );
    assert_eq!(
        events
            .last()
            .and_then(|item| item.end.as_ref())
            .map(|end| end.stop_reason.as_str()),
        Some("timeout"),
        "最终状态不能保留上一轮 cancelled"
    );
}
