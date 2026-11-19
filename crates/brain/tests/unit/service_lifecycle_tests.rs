use super::*;

/// 场景：适配器未启动成功，也不能在活动记录中永久显示“进行中”。
/// 数据构造：指向不存在程序的真实服务，提交一条请求。
/// 执行过程：等待 BackendDown 后停止并收集同轮 Trace。
/// 预期结果：恰好一个 Error 和 error Turn，没有伪造已发给模型的 Prompt。
#[test]
fn failed_session_start_finishes_without_faking_a_sent_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let config = BrainConfig::new(
        Backend {
            kind: BackendKind::ClaudeCode,
            program: dir.path().join("missing-adapter"),
            args: vec![],
        },
        NORMA.into(),
        dir.path().into(),
        dir.path().join("state"),
    );
    let (service, status) = BrainService::start(config);
    service.push(
        BrainEventKind::UserRequest {
            text: "启动故障".into(),
        }
        .in_scope(test_scope()),
    );
    let mut traces = Vec::new();
    loop {
        match status
            .recv_timeout(Duration::from_secs(5))
            .expect("启动必须给出错误")
            .kind
        {
            BrainStatusKind::Trace(t) => traces.push(t),
            BrainStatusKind::BackendDown { .. } => break,
            _ => {}
        }
    }
    service.shutdown();
    traces.extend(status.try_iter().filter_map(|s| match s.kind {
        BrainStatusKind::Trace(t) => Some(t),
        _ => None,
    }));
    assert_eq!(
        traces.iter().map(|t| t.item.kind).collect::<Vec<_>>(),
        vec![TraceKind::Error, TraceKind::Turn],
        "创建失败也必须收口，但不伪造 Prompt"
    );
    assert!(
        traces
            .iter()
            .all(|t| t.turn == traces[0].turn && t.turn > 0),
        "错误和结束关联同一轮"
    );
    assert_eq!(
        traces[1].item.end.as_ref().unwrap().stop_reason,
        "error",
        "保存失败状态"
    );
}

/// 场景：切换后端会停止尚未完成的 prompt，活动记录仍须有取消结束。
/// 数据构造：真实子进程假 ACP 收到 FAKE_SLEEP 后延迟回复，保持接收通道。
/// 执行过程：等适配器收到输入后 shutdown，排空服务状态。
/// 预期结果：恰好一条 cancelled Turn，与输入同轮，没有成功结束。
#[test]
fn shutdown_finishes_the_inflight_turn() {
    let h = start();
    h.push(BrainEventKind::UserRequest {
        text: "FAKE_PARTIAL_SLEEP".into(),
    });
    h.wait_log("partial-output-sent");
    h.service.shutdown();
    let traces: Vec<_> = h
        .status
        .try_iter()
        .filter_map(|s| match s.kind {
            BrainStatusKind::Trace(t) => Some(t),
            _ => None,
        })
        .collect();
    let prompt = traces
        .iter()
        .find(|t| t.item.kind == TraceKind::Prompt)
        .expect("收到输入");
    let ends: Vec<_> = traces
        .iter()
        .filter(|t| t.item.kind == TraceKind::Turn)
        .collect();
    assert_eq!(
        ends.len(),
        1,
        "取消必须给本轮一个结束，不可永远只剩输入：{traces:?}"
    );
    assert_eq!(ends[0].turn, prompt.turn, "结束与输入关联同轮");
    assert_eq!(
        ends[0].item.end.as_ref().unwrap().stop_reason,
        "cancelled",
        "取消不是成功"
    );
    let reply = traces
        .iter()
        .find(|t| t.item.kind == TraceKind::Reply)
        .expect("已收到的部分输出必须 flush");
    assert_eq!(reply.item.text, "取消前的部分输出", "不能丢掉缓冲中的回复");
}

/// 场景：同一 App 进程切换后端重新启动服务，不能覆盖旧轮次。
/// 数据构造：依次启动两个 BrainService，各提交首轮用户请求。
/// 执行过程：从公开状态通道取得 Prompt 轮次，随后关闭各服务。
/// 预期结果：两个轮次均有效且不同，不依赖重启次数或 worker 编号。
#[test]
fn restarted_services_do_not_reuse_turn_ids() {
    let mut ids = Vec::new();
    for _ in 0..2 {
        let h = start();
        h.push(BrainEventKind::UserRequest {
            text: "核查进度".into(),
        });
        if let BrainStatusKind::Trace(trace) = h.wait_status(|s| {
            matches!(s,
            BrainStatusKind::Trace(t) if t.item.kind == TraceKind::Prompt)
        }) {
            ids.push(trace.turn);
        }
        h.service.shutdown();
    }
    assert_eq!(ids.len(), 2, "两个服务均收到输入");
    assert!(ids.iter().all(|id| *id > 0), "轮次必须非零");
    assert_ne!(ids[0], ids[1], "重启不能将两个会话的输入、输出并入同轮");
}

/// 场景：默认配置不再给幕僚一轮 prompt 设秒数上限。
///
/// 数据构造：`BrainConfig::new` 只用临时目录和缺失适配器，不改 `prompt_timeout`。
/// 旧默认是 `Duration::from_secs(180)`。
///
/// 执行：只读默认值，不启动会话。
///
/// 预期结果：
///   - 正断言：`prompt_timeout` 为 None，这一轮没有固定截止
///   - 负断言：不是 Some(180 秒)
#[test]
fn default_prompt_timeout_is_unlimited() {
    let dir = tempfile::tempdir().unwrap();
    let config = BrainConfig::new(
        Backend {
            kind: BackendKind::ClaudeCode,
            program: dir.path().join("missing-adapter"),
            args: vec![],
        },
        NORMA.into(),
        dir.path().into(),
        dir.path().join("state"),
    );
    assert!(
        config.prompt_timeout.is_none(),
        "默认必须不限时，不能再写成任何秒数"
    );
    assert_ne!(
        config.prompt_timeout,
        Some(Duration::from_secs(180)),
        "180 秒上限必须从默认配置去掉"
    );
}

/// 场景：不传超时时，慢于旧 200 毫秒探测窗的部分输出仍能正常结束；中途取消记为 cancelled，不是 timeout。
///
/// 数据构造：
///   FAKE_PARTIAL_SLEEP 先发「取消前的部分输出」，再睡 600 毫秒（fake_acp.py 的 time.sleep(0.6)），
///   然后以 end_turn 结束。显式超时测试仍用 200 毫秒，会在这 600 毫秒里被掐掉。
///   本测试传 None，不设截止。取消用例在进度回调看到这段文字后把 AtomicBool 设为 true。
///
/// 执行过程：
///   1. prompt_observed(..., None) 等到适配器自己结束
///   2. 另一次同样的 prompt，收到部分输出后置取消位
///
/// 预期结果：
///   - 不限时：Ok，正文同时含「取消前的部分输出」和随后的「收到」，stop_reason 是 end_turn
///   - 不限时：不是 Timeout，stop_reason 不是 timeout
///   - 取消：Err(Cancelled)，stop_reason 是 cancelled
///   - 取消：不是 Timeout，也不是 end_turn
#[test]
fn unlimited_prompt_finishes_slow_turn_and_cancel_stays_cancelled() {
    use brain::acp::client::{AcpError, AcpSession, SessionOptions};
    use brain::acp::trace::TraceKind;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let dir = tempfile::tempdir().unwrap();
    let options = SessionOptions {
        kind: BackendKind::ClaudeCode,
        program: "python3".into(),
        args: vec![Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/support/fake_acp.py")
            .display()
            .to_string()],
        cwd: dir.path().into(),
        env: vec![("CLAUDE_CONFIG_DIR".into(), dir.path().display().to_string())],
        meta: None,
        mode: None,
        model: None,
        effort: None,
        staff_shell: None,
        handshake_timeout: Duration::from_secs(5),
    };
    let mut session = AcpSession::start(&options).expect("建立真实 ACP 管道");
    let mut items = Vec::new();
    let result = session.prompt_observed(
        "FAKE_PARTIAL_SLEEP",
        &[],
        &AtomicBool::new(false),
        None,
        &mut |item| items.push(item),
        &mut |_| {},
    );
    let reply = result.expect("不限时不能把 600 毫秒的部分输出收成超时");
    assert!(
        reply.contains("取消前的部分输出"),
        "超时前已经流到的文字必须留在成功回复里，实际：{reply}"
    );
    assert!(
        reply.contains("收到"),
        "适配器睡完后的收尾文字也必须到达，说明没有中途 abort，实际：{reply}"
    );
    let ends: Vec<_> = items.iter().filter_map(|item| item.end.as_ref()).collect();
    assert_eq!(ends.len(), 1, "不限时这一轮只能有一个结束记录");
    assert_eq!(ends[0].stop_reason, "end_turn", "应由适配器正常结束");
    assert_ne!(ends[0].stop_reason, "timeout", "不限时不能记成 timeout");

    let mut session = AcpSession::start(&options).expect("取消用例再建立一条管道");
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let mut items = Vec::new();
    let result = session.prompt_observed(
        "FAKE_PARTIAL_SLEEP",
        &[],
        cancel.as_ref(),
        None,
        &mut |item| items.push(item),
        &mut |item| {
            if item.kind == TraceKind::Reply && item.text == "取消前的部分输出" {
                flag.store(true, Ordering::Relaxed);
            }
        },
    );
    assert!(
        matches!(result, Err(AcpError::Cancelled)),
        "用户取消必须是 Cancelled，实际：{result:?}"
    );
    assert!(
        !matches!(result, Err(AcpError::Timeout)),
        "取消不能被记成超时"
    );
    let ends: Vec<_> = items.iter().filter_map(|item| item.end.as_ref()).collect();
    assert_eq!(ends.len(), 1, "取消这一轮只能有一个结束记录");
    assert_eq!(ends[0].stop_reason, "cancelled", "停止原因必须是 cancelled");
    assert_ne!(
        ends[0].stop_reason, "timeout",
        "取消的停止原因不能是 timeout"
    );
    assert_ne!(ends[0].stop_reason, "end_turn", "取消不能被记成正常结束");
}

/// 场景：适配器断连、超时与远端取消不能遗留进行中轮次，也不能被记成成功。
/// 数据构造：真实 ACP 子进程分别崩溃、发送部分输出后延迟、返回 cancelled。
/// 执行过程：通过公开 AcpSession 调用 prompt，收集状态出口。
/// 预期结果：每次恰好一个相应结束；超时前已接收的文字保留；返回值均为错误。
#[test]
fn every_abnormal_prompt_exit_finishes_once() {
    use brain::acp::client::{AcpSession, SessionOptions};
    let dir = tempfile::tempdir().unwrap();
    let options = SessionOptions {
        kind: BackendKind::ClaudeCode,
        program: "python3".into(),
        args: vec![Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/support/fake_acp.py")
            .display()
            .to_string()],
        cwd: dir.path().into(),
        env: vec![("CLAUDE_CONFIG_DIR".into(), dir.path().display().to_string())],
        meta: None,
        mode: None,
        model: None,
        effort: None,
        staff_shell: None,
        handshake_timeout: Duration::from_secs(5),
    };
    for (text, stop) in [
        ("FAKE_CRASH", "error"),
        ("FAKE_PARTIAL_SLEEP", "timeout"),
        ("FAKE_REMOTE_CANCEL", "cancelled"),
    ] {
        let mut session = AcpSession::start(&options).expect("建立真实 ACP 管道");
        let mut items = Vec::new();
        let result = session.prompt(
            text,
            &std::sync::atomic::AtomicBool::new(false),
            Duration::from_millis(200),
            &mut |item| items.push(item),
        );
        assert!(result.is_err(), "{text} 不应作为成功回复");
        let ends: Vec<_> = items.iter().filter_map(|i| i.end.as_ref()).collect();
        assert_eq!(ends.len(), 1, "{text} 必须恰好一个结束");
        assert_eq!(ends[0].stop_reason, stop, "{text} 的停止原因准确");
        if stop == "timeout" {
            assert!(
                items
                    .iter()
                    .any(|i| i.kind == TraceKind::Reply && i.text == "取消前的部分输出"),
                "超时前已收到的输出不能留在合并器中丢失"
            );
        }
    }
}

/// 场景：并行审批 worker 名称相同，两个输入不得共用持久化轮次键。
/// 数据构造：两个 worker 同时各处理一项延迟审批；收集两条 Prompt。
/// 执行过程：从服务公开事件流取审批轮次再停止服务。
/// 预期结果：两条都是 Approval 且轮次不同，不与服务内局部编号绑定。
#[test]
fn parallel_approval_workers_have_distinct_turns() {
    let h = start_with(2);
    h.push(approval("unique-a", "cargo test FAKE_SLEEP"));
    h.push(approval("unique-b", "cargo test FAKE_SLEEP"));
    let mut ids = Vec::new();
    for _ in 0..2 {
        if let BrainStatusKind::Trace(t) = h.wait_status(|s| {
            matches!(s,
            BrainStatusKind::Trace(t) if t.item.kind == TraceKind::Prompt)
        }) {
            assert_eq!(t.lane, "Approval", "两条均来自审批 worker");
            ids.push(t.turn);
        }
    }
    assert_eq!(ids.len(), 2, "收到两条输入");
    assert_ne!(ids[0], ids[1], "同名并行 worker 不得合并活动记录");
    h.service.shutdown();
}
