use super::*;

/// 场景：Claude 额度错误经 ACP 正文与 error 响应返回，不能只剩输入或成功结束。
/// 数据构造：使用安装版 0.94.0 consumer 回放得到的帧形状，不访问账户服务。
/// 执行过程：真实 BrainService/子进程/JSON-RPC 通道处理错误，再提交下一轮。
/// 预期结果：错误正文、结构化错误类别与唯一 error 结束同轮；重建会话的新轮成功且不串轮。
#[test]
fn quota_error_is_readable_finishes_once_and_does_not_poison_the_next_turn() {
    let h = start();
    h.service.push(BrainEvent::UserRequest {
        text: "FAKE_QUOTA".into(),
    });
    let mut traces = Vec::new();
    loop {
        match h
            .status
            .recv_timeout(Duration::from_secs(5))
            .expect("额度错误必须返回")
        {
            BrainStatus::Trace(t) => traces.push(t),
            BrainStatus::BackendDown { reason } => {
                assert!(
                    reason.contains("rate_limit"),
                    "结构化错误类别不能丢失：{reason}"
                );
                break;
            }
            _ => {}
        }
    }
    let turn = traces
        .iter()
        .find(|t| t.item.kind == TraceKind::Prompt)
        .expect("有输入")
        .turn;
    assert!(
        traces.iter().all(|t| t.turn == turn && t.lane == "Main"),
        "所有事件属于原轮次"
    );
    let ends: Vec<_> = traces.iter().filter_map(|t| t.item.end.as_ref()).collect();
    assert_eq!(ends.len(), 1, "额度错误只能结束一次");
    assert_eq!(ends[0].stop_reason, "error", "不得记成成功或永久进行中");
    let body = "You've hit your session limit · resets 6:30am (America/Los_Angeles)";
    assert!(
        traces
            .iter()
            .any(|t| t.item.kind == TraceKind::Reply && t.item.text == body),
        "完整额度正文保留"
    );
    assert!(
        traces.iter().any(|t| t.item.kind == TraceKind::Error
            && t.item.text.contains(body)
            && t.item.text.contains("rate_limit")),
        "错误事件保存正文及原因"
    );
    assert!(
        !traces.iter().any(|t| t.item.kind == TraceKind::Permission),
        "额度错误不伪造工具权限事件"
    );
    h.service.push(BrainEvent::UserRequest {
        text: "下一轮正常请求".into(),
    });
    let next =
        h.wait_status(|s| matches!(s, BrainStatus::Trace(t) if t.item.kind == TraceKind::Turn));
    let BrainStatus::Trace(next) = next else {
        panic!("预期新轮结束")
    };
    assert_ne!(next.turn, turn, "恢复后的轮次不得覆盖额度错误");
    assert_eq!(
        next.item.end.unwrap().stop_reason,
        "end_turn",
        "下一轮不继承旧错误"
    );
    assert_eq!(
        h.log().matches("session/new").count(),
        2,
        "额度错误后须重新建立会话，不能只换轮次号"
    );
    h.service.shutdown();
}
