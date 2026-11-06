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
    let service = Running(Some(service));
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

fn scoped_config(dir: &Path) -> BrainConfig {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_acp.py");
    let mut config = BrainConfig::new(
        Backend {
            kind: BackendKind::ClaudeCode,
            program: "python3".into(),
            args: vec![script.display().to_string()],
        },
        "/opt/norma/bin/norma".into(),
        dir.into(),
        dir.join("state"),
    );
    config.router.summary_every = None;
    config.env = vec![("CLAUDE_CONFIG_DIR".into(), dir.display().to_string())];
    config
}

fn user(text: &str) -> BrainEventKind {
    BrainEventKind::UserRequest { text: text.into() }
}

/// 释放项目必须取消主lane和审批lane中的在途任务及排队任务，其他项目仍继续。
/// 数据：alpha在两个lane各持有一个FAKE_HOLD；alpha/beta随后各排队一项。
/// 执行：观察真实ACP Prompt后release alpha，不写释放文件；再发一个迟到alpha事件。
/// 预期：两个在途alpha均取消，排队/迟到alpha不发给模型，beta两项均完成。
#[test]
fn releasing_scope_cancels_both_lanes_without_stopping_other_projects() {
    use std::cell::Cell;
    let dir = tempfile::tempdir().expect("隔离ACP目录");
    let mut config = scoped_config(dir.path());
    let log = dir.path().join("prompts.log");
    config
        .env
        .push(("FAKE_LOG".into(), log.display().to_string()));
    config.env.push((
        "FAKE_RELEASE".into(),
        dir.path().join("never-release").display().to_string(),
    ));
    let (service, statuses) = BrainService::start(config);
    let service = Running(Some(service));
    service.push(user("FAKE_HOLD alpha active").in_scope(scope("alpha")));
    let mut seen = receive_until(
        &statuses,
        |s| matches!(&s.kind, BrainStatusKind::Trace(t) if t.lane == "Main" && t.item.kind == TraceKind::Prompt),
    );
    let mut active_approval = approval();
    if let BrainEventKind::ApprovalRequested(request) = &mut active_approval {
        request.id = "active".into();
        request.subject.command = Some("cargo test FAKE_HOLD".into());
    }
    service.push(active_approval.in_scope(scope("alpha")));
    seen.extend(receive_until(&statuses, |s| matches!(&s.kind, BrainStatusKind::Trace(t) if t.lane == "Approval" && t.item.kind == TraceKind::Prompt)));
    wait_for(
        || {
            std::fs::read_to_string(&log).is_ok_and(|text| {
                text.lines()
                    .filter(|line| line.starts_with("prompt:") && line.contains("FAKE_HOLD"))
                    .count()
                    == 2
            })
        },
        "两个真实子进程都已收到在途请求",
    );
    service.push(user("alpha queued").in_scope(scope("alpha")));
    service.push(approval().in_scope(scope("alpha")));
    service.push(user("beta proceeds").in_scope(scope("beta")));
    service.push(approval().in_scope(scope("beta")));
    service.release_scope(&scope("alpha"));
    service.push(user("alpha late").in_scope(scope("alpha")));
    let main_done = Cell::new(false);
    let approval_done = Cell::new(false);
    seen.extend(receive_until(&statuses, |s| {
        if s.scope == scope("beta") {
            if matches!(&s.kind, BrainStatusKind::Trace(t) if t.lane == "Main" && t.item.kind == TraceKind::Turn) { main_done.set(true); }
            if matches!(&s.kind, BrainStatusKind::ApprovalTurnEnded {id} if id == "same-request") { approval_done.set(true); }
        }
        main_done.get() && approval_done.get()
    }));
    service.shutdown();
    seen.extend(statuses.try_iter());
    let prompts: Vec<_> = seen
        .iter()
        .filter(
            |s| matches!(&s.kind, BrainStatusKind::Trace(t) if t.item.kind == TraceKind::Prompt),
        )
        .collect();
    assert_eq!(
        prompts.len(),
        4,
        "仅两个alpha在途和两个beta任务，alpha排队及迟到事件被丢弃"
    );
    assert_eq!(
        prompts.iter().filter(|s| s.scope == scope("alpha")).count(),
        2,
        "alpha没有再次执行"
    );
    assert_eq!(
        prompts.iter().filter(|s| s.scope == scope("beta")).count(),
        2,
        "beta两个lane均继续"
    );
    let cancelled = seen.iter().filter(|s| s.scope == scope("alpha") && matches!(&s.kind, BrainStatusKind::Trace(t) if t.item.end.as_ref().is_some_and(|end| end.stop_reason == "cancelled"))).count();
    assert_eq!(cancelled, 2, "alpha两个真实在途轮次必须有取消结束证据");
    let beta_ends: Vec<_> = seen
        .iter()
        .filter_map(|s| {
            if s.scope != scope("beta") {
                return None;
            }
            match &s.kind {
                BrainStatusKind::Trace(t) => t
                    .item
                    .end
                    .as_ref()
                    .map(|end| (t.lane.as_str(), end.stop_reason.as_str())),
                _ => None,
            }
        })
        .collect();
    for lane in ["Main", "Approval"] {
        assert_eq!(
            beta_ends
                .iter()
                .filter(|(source, _)| *source == lane)
                .copied()
                .collect::<Vec<_>>(),
            vec![(lane, "end_turn")],
            "beta的{lane}必须恰好一次成功结束，不能以错误或取消假通过"
        );
    }
    assert!(
        !seen
            .iter()
            .any(|s| s.scope == scope("beta")
                && matches!(&s.kind, BrainStatusKind::BackendDown { .. })),
        "beta后端不能受alpha取消影响"
    );

    assert!(!seen.iter().any(|s| s.scope == scope("alpha") && matches!(&s.kind, BrainStatusKind::Trace(t) if t.item.end.as_ref().is_some_and(|end| end.stop_reason == "end_turn"))), "取消不能被记录成成功");
}

fn process_exists(pid: &str) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", pid])
        .output()
        .expect("读取测试子进程存活状态")
        .status
        .success()
}

fn wait_for(predicate: impl Fn() -> bool, message: &str) {
    // 3s 是取消/子进程退出的故障上界；每次都观察实际条件，不用固定sleep推断顺序。
    let deadline = Instant::now() + Duration::from_secs(3);
    while !predicate() {
        assert!(Instant::now() < deadline, "{message}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// 项目关闭发生在ACP握手期间，也必须结束该子进程并让其他项目继续。
/// 数据：脚本把自身PID写入隔离文件，然后在initialize前等待文件信号。
/// 执行：确认alpha子进程活着后释放scope，观察其退出，再放行beta的握手。
/// 预期：alpha在3秒上界内退出（默认握手60秒），无alpha Prompt，beta真实完成。
#[test]
fn scope_release_cancels_a_blocked_adapter_handshake() {
    let dir = tempfile::tempdir().expect("隔离握手测试");
    let pids = dir.path().join("pids");
    let release = dir.path().join("handshake-release");
    let mut config = scoped_config(dir.path());
    config.env.extend([
        ("FAKE_PID_LOG".into(), pids.display().to_string()),
        (
            "FAKE_HANDSHAKE_RELEASE".into(),
            release.display().to_string(),
        ),
    ]);
    let (service, statuses) = BrainService::start(config);
    let service = Running(Some(service));
    service.push(user("alpha handshake").in_scope(scope("alpha")));
    wait_for(
        || std::fs::read_to_string(&pids).is_ok_and(|text| !text.trim().is_empty()),
        "实际子进程应报告PID",
    );
    let pid = std::fs::read_to_string(&pids)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .to_owned();
    assert!(pid.parse::<u32>().is_ok(), "夹具报告有效PID");
    assert!(process_exists(&pid), "取消前必须确认真实子进程活着");
    service.release_scope(&scope("alpha"));
    service.push(user("beta after handshake cancellation").in_scope(scope("beta")));
    wait_for(
        || !process_exists(&pid),
        "关闭项目必须取消正在握手的旧子进程",
    );
    std::fs::write(release, "release beta").unwrap();
    let mut seen = receive_until(&statuses, |s| {
        s.scope == scope("beta")
            && matches!(&s.kind, BrainStatusKind::Trace(t) if t.item.kind == TraceKind::Turn)
    });
    service.shutdown();
    seen.extend(statuses.try_iter());
    assert!(
        !seen.iter().any(|s| s.scope == scope("alpha")
            && matches!(&s.kind, BrainStatusKind::Trace(t) if t.item.kind == TraceKind::Prompt)),
        "握手取消不能声称已向alpha发出Prompt"
    );
    assert!(seen.iter().any(|s| s.scope == scope("beta") && matches!(&s.kind, BrainStatusKind::Trace(t) if t.item.end.as_ref().is_some_and(|end| end.stop_reason == "end_turn"))), "beta正常完成");
}

struct Running(Option<BrainService>);
impl std::ops::Deref for Running {
    type Target = BrainService;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("running service")
    }
}
impl Running {
    fn shutdown(mut self) {
        self.0.take().expect("running service").shutdown();
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        if let Some(service) = self.0.take() {
            service.shutdown();
        }
    }
}
