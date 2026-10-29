#![cfg(target_os = "macos")]
use norma::protocol::{QuestionAnswer, QuestionState};
use runtime::{AgentHookRuntimeConfig, NodeInput, NodeView, Runtime};
use serde_json::json;
use std::time::{Duration, Instant};

struct Owned {
    runtime: Runtime,
    model: std::process::Child,
}
impl Drop for Owned {
    fn drop(&mut self) {
        self.runtime.kill_all();
        let _ = self.model.kill();
        let _ = self.model.wait();
    }
}
fn tick(runtime: &mut Runtime) {
    runtime.drain_all();
    runtime.drain_native_question_hosts("native-fixture");
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

/// 场景：默认生产 Runtime 原 TUI 的三题，经真实 OpenCode 工具与本地确定性模型闭环。
/// 构造：临时数据/配置、真实 CLI、本机模型 fixture，包含单选、多选和自定义三题。
/// 执行：原 TUI 发送有限任务，登记题目后用 Runtime 采用三题答案，检查模型实际收到的工具结果。
/// 预期：精确原工具 Answered；三类答案均进入模型请求；原 TUI 更新；无重发且 Drop 回收。
#[test]
#[ignore = "requires installed OpenCode 1.19.3 and python3; uses only localhost model"]
fn default_native_opencode_fixture_consumes_three_answer_types() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/support/questions/opencode_model.py");
    let child = std::process::Command::new("python3")
        .arg(script)
        .arg(root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut owned = Owned {
        runtime: Runtime::new_for_workspace(root),
        model: child,
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join("port").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let port = std::fs::read_to_string(root.join("port")).unwrap();
    // Ignored integration test runs alone; only its children inherit these isolated settings.
    for (key, path) in [
        ("OPENCODE_DB", "isolated.db"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        std::env::set_var(key, root.join(path));
    }
    std::env::set_var("OPENCODE_DISABLE_AUTOUPDATE", "1");
    std::env::set_var("OPENCODE_CONFIG_CONTENT",json!({"model":"fixture/question","permission":{"*":"deny","question":"allow"},"provider":{"fixture":{"name":"Local fixture","npm":"@ai-sdk/openai-compatible","options":{"baseURL":format!("http://127.0.0.1:{port}/v1"),"apiKey":"local-fixture"},"models":{"question":{"name":"Question fixture","limit":{"context":32000,"output":4096}}}}}}).to_string());
    let runtime = &mut owned.runtime;
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
        "默认生产入口必须绑定原session：{}",
        screen(runtime, node)
    );
    runtime.send_agent_task_text(node,"Bounded native fixture test. Ask the three provided questions once and wait. No other tools or work.").unwrap();
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
    let deadline = Instant::now() + Duration::from_secs(50);
    while runtime.questions().is_empty() && Instant::now() < deadline {
        tick(runtime);
    }
    let questions = runtime.questions();
    assert_eq!(
        questions.len(),
        1,
        "真实工具必须登记一个三题请求：{}",
        screen(runtime, node)
    );
    let question = &questions[0].question;
    assert_eq!(question.items.len(), 3, "所有三题均保留");
    let answers = vec![
        QuestionAnswer {
            key: "question-0".into(),
            selected: vec!["option-1".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-1".into(),
            selected: vec!["option-0".into(), "option-2".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-2".into(),
            selected: vec![],
            text: Some("marker".into()),
        },
    ];
    let submission = runtime
        .claim_question_answer(&question.target, answers, "native-fixture-delivery")
        .unwrap()
        .unwrap();
    runtime.deliver_native_question(&submission).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        tick(runtime);
        if runtime.question_snapshot(&question.target).unwrap().state == QuestionState::Answered
            && screen(runtime, node).contains("NATIVE_FIXTURE_RECEIVED")
        {
            break;
        }
    }
    let snapshot = runtime.question_snapshot(&question.target).unwrap();
    assert_eq!(
        snapshot.state,
        QuestionState::Answered,
        "同工具消费才可成功：{snapshot:?}，{}",
        screen(runtime, node)
    );
    let consumed = std::fs::read_to_string(root.join("consumed.json")).unwrap();
    for value in ["Green", "macOS", "Linux", "marker"] {
        assert!(
            consumed.contains(value),
            "后续模型请求必须实际收到{value}：{consumed}"
        );
    }
    assert!(
        screen(runtime, node).contains("NATIVE_FIXTURE_RECEIVED"),
        "原 TUI 必须同步模型回复：{}",
        screen(runtime, node)
    );
    assert!(
        runtime.deliver_native_question(&submission).is_err(),
        "完成后不重发"
    );
    println!(
        "NATIVE_OPENCODE_FIXTURE_RECEIPT {}",
        serde_json::to_string(&snapshot).unwrap()
    );
}
