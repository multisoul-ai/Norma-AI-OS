use norma::opencode::{client::OpenCodeQuestionClient, native_questions::OpenCodeQuestionHost};
use norma::protocol::{QuestionAnswer, QuestionState};
use runtime::{AgentHookRuntimeConfig, Runtime};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
#[path = "support/opencode_foreground.rs"]
mod support;
use support::*;

fn wait(runtime: &mut Runtime, predicate: impl Fn(&Runtime) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while !predicate(runtime) {
        runtime.drain_native_question_hosts("p");
        assert!(
            Instant::now() < deadline,
            "必须完成指定原生会话/回执状态迁移:{:?}",
            runtime.questions()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
/// 场景：原 A 的答案回包丢失后，原 TUI 切 B；A 后来消费不能丢回执或影响 B 的待答状态。
/// 构造：原 Runtime 卡片、鉴权实例两个同题会话、精确 route 证据与 echo PTY。
/// 执行：默认宿主接入 A、发送一次、切 B、观察 A 的迟到原工具消费。
/// 预期：A 记 ConsumedUnattributed 且无 delivery 归属；B 仍是当前 Pending 并阻断普通任务；仅一个 POST。
#[test]
fn runtime_tracks_foreground_and_settles_retired_original_receipt() {
    let server = Server::new(true);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("route.json");
    route(&path, 1, Some("ses_a"));
    let mut runtime = Runtime::new_for_workspace("/tmp/original");
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:9/agent-hooks".into(),
        token: "test".into(),
        workspace_id: "w".into(),
        workspace_route_id: "r".into(),
        norma_socket: "/tmp/unused.sock".into(),
    }));
    runtime.set_pty_factory(Box::new(|_, _, _, sender| {
        Ok(Box::new(terminal::TestPtyBackend::echo(sender)))
    }));
    let node = runtime
        .spawn_agent(
            canvas::AgentProfile::OpenCode,
            canvas::CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .unwrap();
    let process = runtime.mobile_agent_run_id(node).unwrap().to_owned();
    let client = OpenCodeQuestionClient::new(
        &server.endpoint,
        "/tmp/original",
        "ses_a",
        "opencode",
        "secret",
    )
    .unwrap();
    let host =
        OpenCodeQuestionHost::start_following(client, node, process, path.clone(), "owned".into())
            .unwrap();
    runtime.adopt_opencode_question_host(host, "p").unwrap();
    wait(&mut runtime, |r| {
        r.questions()
            .iter()
            .any(|q| q.question.target.session_id == "ses_a" && q.state == QuestionState::Pending)
    });
    let target = runtime.questions()[0].question.target.clone();
    assert!(runtime.question_is_current(&target), "A 是当前原问题");
    let answers = vec![
        QuestionAnswer {
            key: "question-0".into(),
            selected: vec!["option-1".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-1".into(),
            selected: vec!["option-0".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-2".into(),
            selected: vec![],
            text: Some("marker".into()),
        },
    ];
    let submission = runtime
        .claim_question_answer(&target, answers, "once")
        .unwrap()
        .unwrap();
    runtime.deliver_native_question(&submission).unwrap();
    wait(&mut runtime, |r| {
        r.question_snapshot(&target).unwrap().state == QuestionState::Unknown
    });
    route(&path, 2, Some("ses_b"));
    wait(&mut runtime, |r| {
        r.questions()
            .iter()
            .any(|q| q.question.target.session_id == "ses_b" && q.state == QuestionState::Pending)
    });
    assert!(
        !runtime.question_is_current(&target),
        "B 当前时旧 A 只能对账，不能委托/提交"
    );
    assert_eq!(
        runtime.agent_session_id(node).as_deref(),
        Some("ses_b"),
        "卡片跟随原 TUI 的可信会话"
    );
    server.consumed.store(true, Ordering::Release);
    wait(&mut runtime, |r| {
        r.question_snapshot(&target).unwrap().state == QuestionState::ConsumedUnattributed
    });
    assert!(
        runtime
            .question_snapshot(&target)
            .unwrap()
            .delivery_id
            .is_none(),
        "未知发送不能冒认我方完成"
    );
    assert!(
        runtime.has_pending_question(node),
        "A 的迟到消费不能解除 B 的问题占有"
    );
    assert!(
        runtime
            .send_agent_task_text(node, "must not enter question")
            .is_err(),
        "下一步不能粘贴到 B 的问答控件"
    );
    assert_eq!(
        server.posts.load(Ordering::Acquire),
        1,
        "切换和对账均不能重发 POST"
    );
    runtime.kill_all();
}
