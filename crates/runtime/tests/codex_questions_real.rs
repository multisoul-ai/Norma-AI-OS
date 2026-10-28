#![cfg(target_os = "macos")]
use norma::protocol::{QuestionAnswer, QuestionState};
use runtime::{AgentHookRuntimeConfig, NodeInput, NodeView, Runtime};
use std::time::{Duration, Instant};
use storage::question_delivery_store as delivery;

struct Owned(Runtime);
impl Drop for Owned {
    fn drop(&mut self) {
        self.0.kill_all();
    }
}
fn tick(runtime: &mut Runtime) {
    runtime.drain_all();
    runtime.drain_native_question_hosts("native-test");
    std::thread::sleep(Duration::from_millis(20));
}
fn screen(runtime: &Runtime, node: u64) -> String {
    let Some(NodeView::Term { snapshot, .. }) = runtime.node_view(node) else {
        return String::new();
    };
    let total = snapshot.history_size + snapshot.lines;
    runtime
        .node_terminal_rows(node, total.saturating_sub(120), total.saturating_sub(1))
        .unwrap_or_default()
        .iter()
        .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
fn live_screen(runtime: &Runtime, node: u64) -> String {
    let Some(NodeView::Term { snapshot, .. }) = runtime.node_view(node) else {
        return String::new();
    };
    snapshot
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
fn enter(runtime: &mut Runtime, node: u64) {
    runtime
        .send_node_input(
            node,
            NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            }),
        )
        .unwrap();
}
fn submit(runtime: &mut Runtime, node: u64, text: &str) {
    runtime.send_agent_task_text(node, text).unwrap();
    let deadline = Instant::now() + Duration::from_millis(200);
    while Instant::now() < deadline {
        tick(runtime);
    }
    enter(runtime, node);
}

/// 场景：默认桌面生产启动链路回答原生Codex的真实模型问题，不能用第二线程代答。
/// 构造：已信任仓库cwd、独立server/TUI与新会话；真实CLI及既有模型配置；临时回答意图库。
/// 执行：Runtime默认spawn → 原TUI /plan及唯一测试任务 → 取得原题 → 持久化意图 → 提交Green。
/// 预期：同一QuestionRef有精确工具消费，原TUI输出模型确认，重复提交无效；Drop回收自有进程。
#[test]
#[ignore = "requires installed Codex 0.162.1 and configured model; one bounded real question"]
fn default_native_codex_consumes_answer_on_original_question() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn,norma::codex=debug")
        .try_init();
    let cwd = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut owned = Owned(Runtime::new_for_workspace(cwd));
    let runtime = &mut owned.0;
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:9/agent-hooks".into(),
        token: "isolated-test".into(),
        workspace_id: "native-question-test".into(),
        workspace_route_id: "isolated".into(),
        norma_socket: "/tmp/norma-unused-question-test.sock".into(),
    }));
    let node = runtime
        .spawn_agent(
            canvas::AgentProfile::Codex,
            canvas::CanvasRect::new(0., 0., 900., 600.),
            135,
            45,
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(25);
    while Instant::now() < deadline {
        tick(runtime);
        if !runtime.agent_task_input_held(node) && screen(runtime, node).contains("Ask Codex") {
            break;
        }
    }
    assert!(
        screen(runtime, node).contains("Ask Codex"),
        "默认原TUI必须就绪：{}",
        screen(runtime, node)
    );
    assert!(
        !runtime.agent_task_input_held(node),
        "原生宿主交付后才可输入测试任务"
    );
    submit(runtime, node, "/plan");
    let settle = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < settle {
        tick(runtime);
    }
    if live_screen(runtime, node)
        .lines()
        .any(|line| line.trim() == "› /plan")
    {
        enter(runtime, node);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        tick(runtime);
        let view = live_screen(runtime, node);
        if view.contains("Ask Codex") && view.to_ascii_lowercase().contains("plan mode") {
            break;
        }
    }
    assert!(
        live_screen(runtime, node).contains("Ask Codex")
            && live_screen(runtime, node)
                .to_ascii_lowercase()
                .contains("plan mode"),
        "进入原生Plan模式后才能要求澄清工具：{}",
        screen(runtime, node)
    );
    println!("NATIVE_CODEX_STAGE plan_ready");
    submit(runtime,node,"This is a bounded question-answer integration test. Do not inspect files, run commands, or change anything. Call request_user_input exactly once to ask: Which label should the native test use? Offer exactly Blue and Green with short descriptions. Wait for my answer. After receiving it, reply with NATIVE_REPLY: followed by the chosen label. No implementation plan is needed.");
    let deadline = Instant::now() + Duration::from_secs(100);
    while runtime.questions().is_empty() && Instant::now() < deadline {
        tick(runtime);
        assert!(
            !live_screen(runtime, node).contains("Reconnecting to server"),
            "原连接不应断开：{}",
            screen(runtime, node)
        );
    }
    let questions = runtime.questions();
    assert_eq!(
        questions.len(),
        1,
        "默认生产宿主必须登记原生模型问题：{}",
        screen(runtime, node)
    );
    let question = &questions[0].question;
    assert_eq!(question.items.len(), 1, "本次隔离测试只允许一道原问题");
    let option = question.items[0]
        .options
        .iter()
        .find(|option| option.label.starts_with("Green"))
        .expect("模型必须提供要求的Green选项");
    let answers = vec![QuestionAnswer {
        key: question.items[0].key.clone(),
        selected: vec![option.key.clone()],
        text: None,
    }];
    let submission = runtime
        .claim_question_answer(&question.target, answers.clone(), "native-real-delivery")
        .unwrap()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let store = delivery::QuestionDeliveryStore::new(directory.path().join("intent.db"));
    let reservation = store
        .reserve(&delivery::QuestionDeliveryDraft {
            delivery_id: submission.delivery_id().into(),
            target: serde_json::to_value(&question.target).unwrap(),
            question: serde_json::to_value(question).unwrap(),
            answers: serde_json::to_value(&answers).unwrap(),
        })
        .unwrap();
    let delivery::QuestionDeliveryReservation::Claimed(lease) = reservation else {
        panic!("新原题必须首次领取持久化意图")
    };
    runtime.deliver_native_question(&submission).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        tick(runtime);
        let text = screen(runtime, node);
        if runtime.question_snapshot(&question.target).unwrap().state == QuestionState::Answered
            && (text.contains("NATIVE_REPLY: Green") || text.contains("NATIVE_REPLY:Green"))
        {
            break;
        }
    }
    let snapshot = runtime.question_snapshot(&question.target).unwrap();
    assert_eq!(
        snapshot.state,
        QuestionState::Answered,
        "必须收到原call_id的精确工具消费，snapshot={snapshot:?}：{}",
        screen(runtime, node)
    );
    let text = screen(runtime, node);
    assert!(
        text.contains("NATIVE_REPLY: Green") || text.contains("NATIVE_REPLY:Green"),
        "原模型必须在原TUI继续并确认Green：{text}"
    );
    assert!(
        runtime.deliver_native_question(&submission).is_err(),
        "已消费原题不能再次提交"
    );
    store
        .confirm(&lease, snapshot.evidence.as_deref().unwrap())
        .unwrap();
    assert_eq!(
        store
            .get(&serde_json::to_value(&question.target).unwrap())
            .unwrap()
            .unwrap()
            .state,
        delivery::QuestionDeliveryState::Answered,
        "原消费证据才可更新持久化回执"
    );
    println!(
        "NATIVE_CODEX_QUESTION_RECEIPT {}",
        serde_json::to_string(&snapshot).unwrap()
    );
}
