//! 显式运行的真实 OpenCode 回归；使用隔离临时目录和真实 BrainService/TaskStore。
use brain::{Backend, BackendKind, BrainConfig, BrainEventKind, BrainService, BrainStatusKind};
use norma::protocol::{BrainScope, QuestionAnswer, StaffQuestionState, Task, TaskMessage};
use std::time::{Duration, Instant};

/// 场景：真实 OpenCode 原生 question 经产品链显示持久问答卡，回答进入下一轮。
/// 数据构造：隔离任务，仅要求选择 Light/Dark，不提供文件或派活能力。
/// 执行：启动 BrainService，收到 Question 后选择 Dark 并请求下一轮。
/// 预期：首轮有原生题、下一轮回复确认 Dark；不将原工具 decline 误作功能不可用。
#[test]
#[ignore = "requires installed/authenticated OpenCode and a real model"]
fn native_question_roundtrip_through_brain_service() {
    let root = tempfile::tempdir().expect("isolated project");
    let mut config = BrainConfig::new(
        Backend {
            kind: BackendKind::OpenCode,
            program: "/opt/homebrew/bin/opencode".into(),
            args: vec!["acp".into()],
        },
        root.path().join("norma-unavailable"),
        root.path().into(),
        root.path().join("state"),
    );
    config.router.summary_every = None;
    config.prompt_timeout = Some(Duration::from_secs(120));
    let (service, statuses) = BrainService::start(config);
    let store = service.tasks();
    let mut task = Task::new("live-question", "w", "p", "Question regression");
    task.user_message("这是原生问答工具验收。请现在调用 question，询问界面主题 Light 或 Dark。不要调用 shell 或读取文件。收到下一轮主题答案时只回复已选择的主题。当前工具 decline 仅表示答案在下一轮提供，不要重新问。");
    store.create(task).expect("create");
    let request = || {
        BrainEventKind::TaskRequest {
            task_id: "live-question".into(),
        }
        .in_scope(BrainScope {
            workspace_id: "w".into(),
            route_id: "r".into(),
        })
    };
    service.push(request());
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut answered = false;
    let mut confirmed = false;
    let mut started_turns = 0;
    while let Ok(status) = statuses.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        if matches!(
            &status.kind,
            BrainStatusKind::TaskActivity {
                update: brain::event::TaskUpdate::Started,
                ..
            }
        ) {
            started_turns += 1;
        }
        if let BrainStatusKind::TaskChanged(task) = status.kind {
            if !answered {
                if let Some(question) = task
                    .questions
                    .iter()
                    .find(|question| question.state == StaffQuestionState::Pending)
                {
                    let item = &question.items[0];
                    let dark = item
                        .options
                        .iter()
                        .find(|option| option.label.contains("Dark"))
                        .expect("Dark option");
                    println!("REAL QUESTION: {} / {}", item.prompt, dark.label);
                    store
                        .resolve_question(
                            "w",
                            "live-question",
                            &question.id,
                            StaffQuestionState::Answered,
                            &[QuestionAnswer {
                                key: item.key.clone(),
                                selected: vec![dark.key.clone()],
                                text: None,
                            }],
                        )
                        .expect("answer");
                    answered = true;
                    service.push(request());
                }
            } else if started_turns >= 2
                && task
                    .messages
                    .iter()
                    .rev()
                    .take_while(|m| !matches!(m, TaskMessage::Answered { .. }))
                    .any(
                        |m| matches!(m, TaskMessage::Staff(reply) if reply.text().contains("Dark")),
                    )
            {
                confirmed = true;
                println!("REAL REPLY: {:?}", task.messages.last());
                break;
            }
        }
    }
    service.shutdown();
    assert!(answered, "真实 OpenCode 问题必须抵达 TaskStore 并可回答");
    assert!(confirmed, "下一轮真实模型必须读取用户答案 Dark");
}
