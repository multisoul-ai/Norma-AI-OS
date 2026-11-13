use brain::{Backend, BackendKind, BrainConfig, BrainEventKind, BrainService, BrainStatusKind};
use norma::protocol::{BrainScope, QuestionAnswer, StaffQuestionState, Task, TaskMessage};
use std::time::{Duration, Instant};

/// 场景：同一ACP轮次并发两题时全部及时显示；首题答案只在下一轮用户消息中提供。
/// 数据构造：真实stdio假适配器，复用从Claude/Codex抓到的原始form请求，一次发两题再等响应。
/// 执行过程：启动任务 → 等两张持久卡 → 回答第一题、跳过第二题并排入下一轮 → 检查回执与最终回复。
/// 预期结果：无须先答第一题就能看到第二题；工具只收到decline而非答案，下一轮快照包含Dark。
#[test]
fn concurrent_acp_questions_publish_before_answers_and_resume_with_next_user_message() {
    let root = tempfile::tempdir().expect("root");
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let log = root.path().join("replies.json");
    let mut config = BrainConfig::new(
        Backend {
            kind: BackendKind::ClaudeCode,
            program: "python3".into(),
            args: vec![manifest
                .join("tests/support/fake_staff_questions.py")
                .display()
                .to_string()],
        },
        "/opt/norma".into(),
        root.path().into(),
        root.path().join("state"),
    );
    config.router.summary_every = None;
    config.env = vec![
        (
            "CLAUDE_CONFIG_DIR".into(),
            root.path().display().to_string(),
        ),
        (
            "FAKE_QUESTION_FIXTURES".into(),
            manifest.join("tests/fixtures/staff").display().to_string(),
        ),
        ("FAKE_QUESTION_LOG".into(), log.display().to_string()),
    ];
    let (service, statuses) = BrainService::start(config);
    let store = service.tasks();
    let mut task = Task::new("t", "w", "p", "Theme");
    task.user_message("Choose a theme");
    store.create(task).expect("task");
    let request = || {
        BrainEventKind::TaskRequest {
            task_id: "t".into(),
        }
        .in_scope(BrainScope {
            workspace_id: "w".into(),
            route_id: "r".into(),
        })
    };
    service.push(request());
    // 5秒只等待本地stdio握手；两题同时出现是条件，不靠睡眠构造成功。
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut pending = None;
    while let Ok(status) = statuses.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        if let BrainStatusKind::TaskChanged(task) = status.kind {
            if task.pending_question_count() == 2 {
                pending = Some(task);
                break;
            }
        }
    }
    let mut next = None;
    if let Some(task) = &pending {
        let questions: Vec<_> = task
            .messages
            .iter()
            .filter_map(|message| match message {
                TaskMessage::Question { question } => Some(question),
                _ => None,
            })
            .collect();
        let item = &questions[0].items[0];
        let dark = item
            .options
            .iter()
            .find(|option| option.label == "Dark")
            .expect("captured option");
        store
            .resolve_question(
                "w",
                "t",
                &questions[0].id,
                StaffQuestionState::Answered,
                &[QuestionAnswer {
                    key: item.key.clone(),
                    selected: vec![dark.key.clone()],
                    text: None,
                }],
            )
            .expect("answer");
        store
            .resolve_question("w", "t", &questions[1].id, StaffQuestionState::Skipped, &[])
            .expect("skip");
        service.push(request());
        let deadline = Instant::now() + Duration::from_secs(5);
        while let Ok(status) =
            statuses.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            if let BrainStatusKind::TaskChanged(task) = status.kind {
                if task.messages.iter().any(|m| matches!(m, TaskMessage::Norma{text} if text.contains("NEXT TURN:") && text.contains("Dark"))) {
                    next = Some(task); break;
                }
            }
        }
    }
    service.shutdown();
    assert!(
        pending.is_some(),
        "并发两题必须在任何回答前全部发布，不能被首题阻塞"
    );
    assert!(next.is_some(), "下一轮必须从用户消息读取答案");
    let replies: serde_json::Value =
        serde_json::from_slice(&std::fs::read(log).expect("tool replies")).expect("JSON");
    assert_eq!(
        replies.as_array().expect("array").len(),
        2,
        "每个请求恰好一条响应"
    );
    assert!(
        !replies.to_string().contains("Dark"),
        "不能把答案回传挂起工具"
    );
    assert!(
        replies
            .as_array()
            .expect("array")
            .iter()
            .all(|r| r["result"] == serde_json::json!({"action":"decline"})),
        "工具响应只释放挂起"
    );
}
