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
    config.prompt_timeout = Duration::from_secs(120);
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
                update: brain::event::TaskUpdate::Started { .. },
                ..
            }
        ) {
            started_turns += 1;
        }
        if let BrainStatusKind::TaskChanged(task) = status.kind {
            if !answered {
                if let Some(question) = task.messages.iter().find_map(|m| match m {
                    TaskMessage::Question { question }
                        if question.state == StaffQuestionState::Pending =>
                    {
                        Some(question)
                    }
                    _ => None,
                }) {
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
                    .take_while(|m| !matches!(m, TaskMessage::User { .. }))
                    .any(|m| matches!(m, TaskMessage::Norma { text } if text.contains("Dark")))
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

/// 场景：真实 OpenCode 把格式拒绝误报成用户拒绝后，宿主纠正上下文可驱动独立重试。
/// 数据构造：临时 norma 仅记录 argv，没有 App 连接；两个本任务操作组合成 &&。
/// 执行：真实 ACP/模型/Gate；预期：组合从未执行，两个独立操作各执行一次，不再要求授权。
#[test]
#[ignore = "requires installed/authenticated OpenCode and a real model"]
fn command_format_feedback_reaches_real_model() {
    use brain::acp::{
        client::{AcpSession, SessionOptions},
        gate::Gate,
    };
    use std::{os::unix::fs::PermissionsExt, sync::atomic::AtomicBool};
    let root = tempfile::tempdir().expect("isolated command probe");
    let bin = root.path().join("norma");
    std::fs::write(&bin, "#!/usr/bin/env python3\nimport json, pathlib, sys\np=pathlib.Path(__file__).with_name('executed.jsonl')\nwith p.open('a') as f: f.write(json.dumps(sys.argv[1:])+'\\n')\nprint('{\"ok\":true}')\n").expect("probe CLI");
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let options = SessionOptions {
        kind: BackendKind::OpenCode,
        program: "/opt/homebrew/bin/opencode".into(),
        args: vec!["acp".into()],
        cwd: root.path().into(),
        env: brain::backend::session_env(BackendKind::OpenCode),
        meta: None,
        mode: None,
        model: None,
        effort: None,
        handshake_timeout: Duration::from_secs(30),
    };
    let cancel = AtomicBool::new(false);
    let mut session = AcpSession::start_staff_cancellable(&options, &cancel).expect("real session");
    let gate = Gate::for_project(bin.clone(), root.path().into()).for_task(
        BrainScope {
            workspace_id: "w".into(),
            route_id: "r".into(),
        },
        "t".into(),
    );
    let first = format!(
        "'{}' brain --workspace w --route r task phase t active",
        bin.display()
    );
    let second = format!("'{}' brain --workspace w --route r task propose t Chloe --title Review --prompt 'Review code' --why 'Independent review'", bin.display());
    let prompt = format!("This is a command-format recovery integration test with an isolated no-op CLI. First invoke shell with exactly this single string: {first} && {second}\nIf the host sends corrective feedback, follow it. Do not read or write other files or use any other tools. After successful execution say RECOVERED.");
    let mut format_rejected = false;
    let reply = session
        .prompt(
            &prompt,
            &gate,
            &cancel,
            Duration::from_secs(120),
            &mut |item| {
                if item
                    .tool
                    .as_ref()
                    .is_some_and(|tool| tool.command_format_error)
                {
                    format_rejected = true;
                }
                if item.kind == brain::acp::trace::TraceKind::Permission {
                    println!("REAL PERMISSION: {:?}", item.tool);
                }
            },
        )
        .expect("real format recovery");
    println!("REAL FORMAT REPLY: {reply}");
    let calls = std::fs::read_to_string(root.path().join("executed.jsonl"))
        .expect("corrected calls executed");
    let calls: Vec<serde_json::Value> = calls
        .lines()
        .map(|line| serde_json::from_str(line).expect("argv"))
        .collect();
    assert!(
        format_rejected,
        "必须实际触发组合命令拒绝，不能用模型预先拆分冒充通过"
    );
    assert_eq!(calls.len(), 2, "只执行两条纠正命令，每条恰好一次");
    assert_eq!(calls[0][6], "phase", "先推进阶段");
    assert_eq!(calls[1][6], "propose", "再提议下一位 Worker");
    assert!(reply.contains("RECOVERED"), "真实模型应完成纠正并确认");
}
