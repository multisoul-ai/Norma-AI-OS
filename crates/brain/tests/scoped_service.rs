use brain::acp::trace::TraceKind;
use brain::risk::ApprovalSubject;
use brain::{
    ApprovalRequest, Backend, BackendKind, BrainConfig, BrainEvent, BrainEventKind, BrainService,
    BrainStatus, BrainStatusKind,
};
use norma::protocol::BrainScope;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

fn scope(route: &str) -> BrainScope {
    BrainScope {
        workspace_id: format!("workspace-{route}"),
        route_id: route.into(),
    }
}

fn approval() -> BrainEventKind {
    BrainEventKind::ApprovalRequested(ApprovalRequest {
        id: "same-request".into(),
        agent: "Alan".into(),
        subject: ApprovalSubject {
            tool_name: "Bash".into(),
            command: Some("cargo check".into()),
            file_path: None,
            project_root: "/project".into(),
        },
    })
}

fn receive_until(
    status: &Receiver<BrainStatus>,
    predicate: impl Fn(&BrainStatus) -> bool,
) -> Vec<BrainStatus> {
    // 10s 仅为进程启动/调度故障上界；审批由显式文件信号释放，不靠睡眠决定顺序。
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut received = Vec::new();
    loop {
        let item = status
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("真实服务应在10秒内返回目标状态");
        let done = predicate(&item);
        received.push(item);
        if done {
            return received;
        }
    }
}

/// 共享服务必须按 workspace+route 隔离同 ID 审批，所有状态须保留事件的身份。
/// 数据：一条 alpha 慢审批占用唯一 worker；alpha/beta 随后各提交 same-request，再关闭 alpha。
/// 执行：等待慢审批 Prompt，发关闭后用主 lane 回执确认 dispatcher 已处理，再释放审批。
/// 预期：beta 被真实 worker 处理且状态身份准确；已关闭 alpha 不发送，关闭 alpha 不吞 beta。
#[test]
fn scoped_approval_cancellation_cannot_close_another_workspaces_job() {
    let dir = tempfile::tempdir().expect("隔离服务目录");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_acp.py");
    let mut config = BrainConfig::new(
        Backend {
            kind: BackendKind::ClaudeCode,
            program: "python3".into(),
            args: vec![script.display().to_string()],
        },
        "/opt/norma/bin/norma".into(),
        dir.path().into(),
        dir.path().join("state"),
    );
    config.router.summary_every = None;
    config.env = vec![("CLAUDE_CONFIG_DIR".into(), dir.path().display().to_string())];
    let release = dir.path().join("release");
    config
        .env
        .push(("FAKE_RELEASE".into(), release.display().to_string()));
    let (service, statuses) = BrainService::start(config);
    let mut warm = approval();
    if let BrainEventKind::ApprovalRequested(request) = &mut warm {
        request.id = "warm".into();
        request.subject.command = Some("cargo test FAKE_HOLD".into());
    }
    service.push(BrainEvent {
        scope: scope("alpha"),
        kind: warm,
    });
    let mut seen = receive_until(
        &statuses,
        |status| matches!(&status.kind, BrainStatusKind::Trace(t) if t.item.kind == TraceKind::Prompt),
    );
    service.push(BrainEvent {
        scope: scope("alpha"),
        kind: approval(),
    });
    service.push(BrainEvent {
        scope: scope("beta"),
        kind: approval(),
    });
    service.push(BrainEvent {
        scope: scope("alpha"),
        kind: BrainEventKind::ApprovalClosed {
            id: "same-request".into(),
        },
    });
    service.push(BrainEvent {
        scope: scope("beta"),
        kind: BrainEventKind::UserRequest {
            text: "dispatcher barrier".into(),
        },
    });
    seen.extend(receive_until(&statuses, |status| matches!(&status.kind, BrainStatusKind::Trace(t) if t.lane == "Main" && t.item.kind == TraceKind::Turn)));
    std::fs::write(release, "release").expect("dispatcher 已应用关闭事件，释放唯一审批 worker");
    seen.extend(receive_until(&statuses, |status| status.scope == scope("beta") && matches!(&status.kind, BrainStatusKind::ApprovalTurnEnded { id } if id == "same-request")));
    service.shutdown();
    seen.extend(statuses.try_iter());
    let prompts: Vec<_> = seen
        .iter()
        .filter_map(|status| match &status.kind {
            BrainStatusKind::Trace(t)
                if t.lane == "Approval" && t.item.kind == TraceKind::Prompt =>
            {
                Some((status, t))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        prompts.len(),
        2,
        "只有 warm 和 beta，已关闭的 alpha 不得发给 ACP"
    );
    assert_eq!(prompts[0].0.scope, scope("alpha"), "warm 保留原来源");
    assert_eq!(
        prompts[1].0.scope,
        scope("beta"),
        "同 ID 的 beta 仍独立执行"
    );
    assert!(
        prompts[1].1.item.text.contains("workspace-beta"),
        "发给模型的提示含精确来源"
    );
    assert!(
        !prompts[1].1.item.text.contains("workspace-alpha"),
        "复用审批会话不能沿用旧提示身份"
    );
    let beta_turn = prompts[1].1.turn;
    assert!(
        seen.iter()
            .filter(
                |status| matches!(&status.kind, BrainStatusKind::Trace(t) if t.turn == beta_turn)
            )
            .all(|status| status.scope == scope("beta")),
        "beta 的全部 trace 都必须归 beta"
    );
    assert!(!seen.iter().any(|status| status.scope == scope("alpha") && matches!(&status.kind, BrainStatusKind::ApprovalTurnEnded { id } if id == "same-request")), "被丢弃 alpha 不能发另一项目的结束回执");
}
