use brain::acp::gate::{Gate, PermissionRequest, Verdict};
use brain::{Backend, BackendKind, BrainConfig, BrainEventKind, BrainService, BrainStatusKind};
use norma::protocol::{BrainScope, Task, TaskMessage};
use std::time::{Duration, Instant};

/// 任务轮次限制写入目标与播报；同一个主会话串行重放两个任务且回复归属正确。
///
/// 数据构造：ws/route 内 task-a 和 task-b，消息分别 ALPHA_ONLY 与 BETA_ONLY。
/// 执行过程：1. 两个任务入队。2. 等两个 TaskChanged 的 Norma 回复。3. 关闭服务并核对重放和权限。
/// 预期结果：各自消息不混入另一任务；模型原文回复保存；禁止 report、跨 task 与跨 route 写入。
#[test]
fn shared_lane_replays_task_messages_and_scopes_mutations() {
    let dir = tempfile::tempdir().expect("root");
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_acp.py");
    let mut config = BrainConfig::new(
        Backend {
            kind: BackendKind::ClaudeCode,
            program: "python3".into(),
            args: vec![script.display().to_string()],
        },
        "/opt/norma".into(),
        dir.path().into(),
        dir.path().join("state"),
    );
    config.router.summary_every = None;
    let prompt_log = dir.path().join("prompts.log");
    config
        .env
        .push(("FAKE_LOG".into(), prompt_log.display().to_string()));
    config
        .env
        .push(("CLAUDE_CONFIG_DIR".into(), dir.path().display().to_string()));
    let (service, statuses) = BrainService::start(config);
    let store = service.tasks();
    let mut a = Task::new("task-a", "ws", "project", "A");
    let mut b = Task::new("task-b", "ws", "project", "B");
    a.user_message("ALPHA_ONLY");
    b.user_message("BETA_ONLY");
    store.create(a).expect("create A");
    store.create(b).expect("create B");
    let scope = BrainScope {
        workspace_id: "ws".into(),
        route_id: "route".into(),
    };
    service.push(
        BrainEventKind::TaskRequest {
            task_id: "task-a".into(),
        }
        .in_scope(scope.clone()),
    );
    service.push(
        BrainEventKind::TaskRequest {
            task_id: "task-b".into(),
        }
        .in_scope(scope.clone()),
    );
    // 两次 Python 进程调度/ACP 握手总预算 10s；没有真实模型网络调用。
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut completed = std::collections::BTreeSet::new();
    while completed.len() < 2 {
        let status = statuses
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("two task replies before deadline");
        if let BrainStatusKind::TaskChanged(task) = status.kind {
            if task
                .messages
                .iter()
                .any(|message| matches!(message, TaskMessage::Norma { .. }))
            {
                completed.insert(task.id);
            }
        }
    }
    service.shutdown();
    let prompts = std::fs::read_to_string(prompt_log).expect("actual ACP prompt log");
    let turns: Vec<_> = prompts
        .lines()
        .filter(|line| line.starts_with("prompt:"))
        .collect();
    assert_eq!(
        turns.len(),
        2,
        "exactly two task prompts must reach the adapter"
    );
    assert!(
        turns[0].contains("ALPHA_ONLY"),
        "first prompt must replay task A messages"
    );
    assert!(
        !turns[0].contains("BETA_ONLY"),
        "B content must not be included in A prompt"
    );
    assert!(
        turns[1].contains("BETA_ONLY"),
        "second prompt must replay task B messages"
    );
    assert!(
        !turns[1].contains("ALPHA_ONLY"),
        "A content must not be included in B prompt"
    );
    let a = store.get("ws", "task-a").expect("A");
    let b = store.get("ws", "task-b").expect("B");
    assert_eq!(
        a.messages.last(),
        Some(&TaskMessage::Norma {
            text: "收到".into()
        }),
        "ACP reply must return to A"
    );
    assert_eq!(
        b.messages.last(),
        Some(&TaskMessage::Norma {
            text: "收到".into()
        }),
        "ACP reply must return to B"
    );
    assert!(
        !serde_json::to_string(&a.messages)
            .expect("A JSON")
            .contains("BETA_ONLY"),
        "B content must not be appended to A"
    );
    assert!(
        !serde_json::to_string(&b.messages)
            .expect("B JSON")
            .contains("ALPHA_ONLY"),
        "A content must not be appended to B"
    );
    let gate =
        Gate::for_project("/opt/norma".into(), dir.path().into()).for_task(scope, "task-a".into());
    let judge = |command: &str| {
        gate.judge(&PermissionRequest {
            tool: "Bash".into(),
            command: Some(command.into()),
            path: None,
        })
    };
    assert_eq!(
        judge("/opt/norma brain --workspace ws --route route task phase task-a merging"),
        Verdict::Allow,
        "task may advance its own phase"
    );
    assert_ne!(
        judge("/opt/norma brain --workspace ws --route route report --text hi"),
        Verdict::Allow,
        "task must never speak"
    );
    assert_ne!(
        judge("/opt/norma brain --workspace ws --route route task phase task-b done"),
        Verdict::Allow,
        "task cannot write another task"
    );
    assert_ne!(
        judge("/opt/norma brain --workspace other --route route task phase task-a done"),
        Verdict::Allow,
        "task cannot write another workspace"
    );
}
