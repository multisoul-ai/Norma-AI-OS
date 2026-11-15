use brain::{Backend, BackendKind, BrainConfig, BrainEventKind, BrainService};
use norma::protocol::{BrainScope, ReplyStatus, Task, TaskMessage};
use std::path::Path;
use std::time::{Duration, Instant};

struct Harness {
    _dir: tempfile::TempDir,
    service: BrainService,
    release: std::path::PathBuf,
    tokens: std::path::PathBuf,
}

fn start(idle: Option<Duration>) -> Harness {
    let dir = tempfile::tempdir().expect("root");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_acp.py");
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
    let release = dir.path().join("release");
    let tokens = dir.path().join("tokens.log");
    for (key, value) in [
        ("FAKE_LOG", dir.path().join("prompts.log")),
        ("FAKE_RELEASE", release.clone()),
        ("FAKE_TOKEN_LOG", tokens.clone()),
        ("CLAUDE_CONFIG_DIR", dir.path().to_owned()),
    ] {
        config.env.push((key.into(), value.display().to_string()));
    }
    config.staff_tokens = Some(brain::staff::gate::StaffTokens::default());
    if let Some(idle) = idle {
        config.task_idle = idle;
    }
    let (service, statuses) = BrainService::start(config);
    // 状态通道不读也不会阻塞（无界），这里只通过任务库观察结果。
    std::mem::forget(statuses);
    Harness {
        _dir: dir,
        service,
        release,
        tokens,
    }
}

fn scope() -> BrainScope {
    BrainScope {
        workspace_id: "ws".into(),
        route_id: "route".into(),
    }
}

fn add_task(harness: &Harness, id: &str, text: &str) {
    let mut task = Task::new(id, "ws", "project", id);
    task.user_message(text);
    harness.service.tasks().create(task).expect("create task");
}

fn request(harness: &Harness, id: &str) {
    harness
        .service
        .push(BrainEventKind::TaskRequest { task_id: id.into() }.in_scope(scope()));
}

fn reply_status(harness: &Harness, id: &str) -> Option<ReplyStatus> {
    let task = harness.service.tasks().get("ws", id).ok()?;
    task.messages
        .iter()
        .rev()
        .find_map(|message| match message {
            TaskMessage::Staff(reply) => Some(reply.status),
            _ => None,
        })
}

/// 等到任务的最近一条回复变成 `status`，最多 15 秒（两次 Python 进程握手）。
fn wait_for(harness: &Harness, id: &str, status: ReplyStatus) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if reply_status(harness, id) == Some(status) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!(
        "task {id} did not reach {status:?}; now {:?}",
        reply_status(harness, id)
    );
}

fn token_lines(harness: &Harness) -> Vec<String> {
    std::fs::read_to_string(&harness.tokens)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// 场景：两个幕僚任务同时进行，一个被卡住不影响另一个。
/// 数据构造：任务 a 的消息含 FAKE_HOLD（后端一直等到放行文件出现），任务 b 是普通消息。
/// 执行过程：先请求 a 再请求 b；等 b 完成；确认 a 还在进行；创建放行文件；等 a 完成。
/// 预期结果：b 在 a 仍卡住时就完成（并行，而不是排在 a 后面）；两个会话各拿到不同的非空 token。
#[test]
fn different_tasks_run_in_parallel_with_their_own_tokens() {
    let harness = start(None);
    add_task(&harness, "a", "FAKE_HOLD alpha");
    add_task(&harness, "b", "beta");
    request(&harness, "a");
    request(&harness, "b");
    wait_for(&harness, "b", ReplyStatus::Done);
    assert_eq!(
        reply_status(&harness, "a"),
        Some(ReplyStatus::Running),
        "b 完成时 a 应该还卡着，说明两个任务是并行的"
    );
    let tokens = token_lines(&harness);
    assert_eq!(tokens.len(), 2, "两个任务两个会话进程：{tokens:?}");
    assert!(
        tokens.iter().all(|token| !token.is_empty()),
        "每个会话都带 token"
    );
    assert_ne!(tokens[0], tokens[1], "两个会话的 token 不能相同");
    std::fs::write(&harness.release, "go").expect("release");
    wait_for(&harness, "a", ReplyStatus::Done);
    harness.service.shutdown();
}

/// 场景：同一个任务的两条请求要按顺序处理，不能同时开两个会话。
/// 数据构造：任务 a 的第一条消息含 FAKE_HOLD；卡住期间再请求一次 a。
/// 预期结果：卡住期间只有一个会话进程；放行后两条请求都完成，仍然只用同一个会话（token 日志一行）。
#[test]
fn requests_for_one_task_share_one_session_in_order() {
    let harness = start(None);
    add_task(&harness, "a", "FAKE_HOLD alpha");
    request(&harness, "a");
    std::thread::sleep(Duration::from_millis(800));
    request(&harness, "a");
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(token_lines(&harness).len(), 1, "同一任务只能有一个会话进程");
    std::fs::write(&harness.release, "go").expect("release");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let rounds = harness
            .service
            .tasks()
            .get("ws", "a")
            .expect("task")
            .messages
            .iter()
            .filter(|m| matches!(m, TaskMessage::Staff(r) if r.status == ReplyStatus::Done))
            .count();
        if rounds == 2 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "两条请求都应完成，现在 {rounds} 条"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(token_lines(&harness).len(), 1, "两轮复用同一个会话");
    harness.service.shutdown();
}

/// 场景：任务会话空闲后回收，下一轮再起新会话。
/// 数据构造：空闲回收设为 400 毫秒；请求 a 完成后等 1.5 秒再请求一次。
/// 预期结果：第二轮能正常完成，并且是新起的会话（token 日志两行且不同）。
#[test]
fn idle_session_is_recycled_and_restarted() {
    let harness = start(Some(Duration::from_millis(400)));
    add_task(&harness, "a", "first");
    request(&harness, "a");
    wait_for(&harness, "a", ReplyStatus::Done);
    std::thread::sleep(Duration::from_millis(1500));
    harness
        .service
        .tasks()
        .update("ws", "a", |task| {
            task.user_message("second");
            Ok(())
        })
        .expect("second message");
    request(&harness, "a");
    let deadline = Instant::now() + Duration::from_secs(15);
    while token_lines(&harness).len() < 2 {
        assert!(Instant::now() < deadline, "空闲回收后应起新会话");
        std::thread::sleep(Duration::from_millis(50));
    }
    let tokens = token_lines(&harness);
    assert_ne!(tokens[0], tokens[1], "新会话换新 token");
    harness.service.shutdown();
}
