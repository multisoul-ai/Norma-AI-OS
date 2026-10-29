#![cfg(target_os = "macos")]
use norma::protocol::{QuestionAnswer, QuestionState};
use runtime::{AgentHookRuntimeConfig, NodeInput, NodeView, Runtime};
use std::time::{Duration, Instant};

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
    snapshot
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 场景：默认 Runtime OpenCode 卡片回答真实模型原题，并由原工具消费后继续。
/// 构造：临时 cwd、隔离 DB、仅允许 question 的项目配置；使用已有模型配置，不借用凭据。
/// 执行：默认 spawn → 原 TUI 提交一次有限任务 → 原题取 Green → 宿主提交并读取精确回执。
/// 预期：原 session 原 call 完成，原 TUI 显示模型确认；重复提交无效；Drop 回收自有进程。
#[test]
#[ignore = "requires installed OpenCode 1.19.3 and available configured model"]
fn default_native_opencode_consumes_answer_on_original_question() {
    let directory = tempfile::tempdir().unwrap();
    // This ignored test runs in its own process; isolate only its provider database.
    std::env::set_var("OPENCODE_DB", directory.path().join("isolated.db"));
    std::fs::write(
        directory.path().join("opencode.json"),
        r#"{"permission":{"*":"deny","question":"allow"}}"#,
    )
    .unwrap();
    let mut owned = Owned(Runtime::new_for_workspace(directory.path()));
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
            canvas::AgentProfile::OpenCode,
            canvas::CanvasRect::new(0., 0., 900., 600.),
            120,
            42,
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(35);
    while Instant::now() < deadline {
        tick(runtime);
        if !runtime.agent_task_input_held(node)
            && runtime.agent_session_id(node).is_some()
            && screen(runtime, node).contains("Build")
        {
            break;
        }
    }
    assert!(
        runtime.agent_session_id(node).is_some(),
        "默认入口必须附着原会话：{}",
        screen(runtime, node)
    );
    assert!(
        !runtime.agent_task_input_held(node),
        "宿主交付并核验后才可输入"
    );
    runtime.send_agent_task_text(node,"Bounded native question test. Do not inspect files, run commands, or change anything. Call the question tool exactly once asking: Which label should this native test use? Offer exactly Blue and Green. Wait for the real answer. After receiving it, reply NATIVE_REPLY: followed by the chosen label. No other work.").unwrap();
    let settle = Instant::now() + Duration::from_millis(200);
    while Instant::now() < settle {
        tick(runtime);
    }
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
    let deadline = Instant::now() + Duration::from_secs(100);
    while runtime.questions().is_empty() && Instant::now() < deadline {
        tick(runtime);
        let text = screen(runtime, node);
        assert!(
            !text.contains("Free usage exceeded"),
            "已配置模型额度不足：{text}"
        );
    }
    let questions = runtime.questions();
    assert_eq!(
        questions.len(),
        1,
        "原宿主应登记真实问题：{}",
        screen(runtime, node)
    );
    let question = &questions[0].question;
    assert_eq!(question.items.len(), 1, "测试要求一题");
    let item = &question.items[0];
    let green = item
        .options
        .iter()
        .find(|option| option.label == "Green")
        .expect("原题有Green");
    let answers = vec![QuestionAnswer {
        key: item.key.clone(),
        selected: vec![green.key.clone()],
        text: None,
    }];
    let submission = runtime
        .claim_question_answer(&question.target, answers, "native-opencode-delivery")
        .unwrap()
        .unwrap();
    runtime.deliver_native_question(&submission).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        tick(runtime);
        if runtime.question_snapshot(&question.target).unwrap().state == QuestionState::Answered
            && screen(runtime, node).contains("NATIVE_REPLY: Green")
        {
            break;
        }
    }
    let snapshot = runtime.question_snapshot(&question.target).unwrap();
    assert_eq!(
        snapshot.state,
        QuestionState::Answered,
        "同工具精确消费才能成功：{snapshot:?}，{}",
        screen(runtime, node)
    );
    assert!(
        screen(runtime, node).contains("NATIVE_REPLY: Green"),
        "原模型必须继续确认答案：{}",
        screen(runtime, node)
    );
    assert!(
        runtime.deliver_native_question(&submission).is_err(),
        "完成后不得重发"
    );
    println!(
        "NATIVE_OPENCODE_QUESTION_RECEIPT {}",
        serde_json::to_string(&snapshot).unwrap()
    );
}
