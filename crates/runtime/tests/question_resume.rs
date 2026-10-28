use canvas::{AgentProfile, CanvasRect};
use norma::agent_hook::{
    parse_hook_event, AgentHookEvent, HookQuestion, HookQuestionRequest, HookSource,
};
use runtime::{AgentHookRuntimeConfig, AgentTaskState, AgentTaskUpdate, NodeInput, Runtime};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use terminal::{Key, KeyInput, PtyBackend, PtyError};

struct ControlledInput(Arc<AtomicBool>);
impl PtyBackend for ControlledInput {
    fn write(&mut self, _: &[u8]) -> Result<(), PtyError> {
        if self.0.load(Ordering::SeqCst) {
            Err(PtyError::Io(std::io::Error::other("test input rejected")))
        } else {
            Ok(())
        }
    }
    fn resize(&mut self, _: u16, _: u16) -> Result<(), PtyError> {
        Ok(())
    }
    fn kill(&mut self) {}
}

fn session_event(node: u64, run: &str, session: &str, source: &str) -> AgentHookEvent {
    let mut event = parse_hook_event(
        HookSource::Claude,
        node,
        serde_json::json!({
            "hook_event_name":"SessionStart", "session_id":session, "source":source
        }),
    )
    .unwrap();
    event.control_run_id = Some(run.into());
    event
}

/// 场景：只有成功写入普通提示的/resume，才能让已退休会话重新绑定。
/// 构造：同进程先从old切到current；可控PTY模拟Enter失败或原题自定义输入。
/// 执行：输入/resume，再输入Enter，最后重放source=resume的old SessionStart。
/// 预期：写入失败和问答中的文字都不授予恢复资格；当前会话保持不变。
#[test]
fn failed_enter_and_question_text_do_not_authorize_retired_session_resume() {
    for pending_question in [false, true] {
        let mut runtime = Runtime::new_for_workspace("/tmp/resume-question-test");
        runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
            endpoint: "http://127.0.0.1:49152/agent-hooks".into(),
            token: "test".into(),
            workspace_id: "w".into(),
            workspace_route_id: "a".into(),
            norma_socket: "/tmp/resume-test.sock".into(),
        }));
        let fail = Arc::new(AtomicBool::new(false));
        let backend = fail.clone();
        runtime.set_pty_factory(Box::new(move |_, _, _, _| {
            Ok(Box::new(ControlledInput(backend.clone())))
        }));
        let node = runtime
            .spawn_agent(
                AgentProfile::Claude,
                CanvasRect::new(0., 0., 640., 400.),
                80,
                24,
            )
            .unwrap();
        let run = runtime.mobile_agent_run_id(node).unwrap().to_owned();
        for session in ["old", "current"] {
            assert!(
                runtime.observe_mobile_hook(&session_event(node, &run, session, "startup")),
                "同一进程的正常新会话可绑定"
            );
            runtime.apply_agent_task_update(AgentTaskUpdate {
                node_id: node,
                profile: AgentProfile::Claude,
                provider_event: "SessionStart".into(),
                to: AgentTaskState::Ready,
                task_title: None,
                session_id: Some(session.into()),
            });
        }
        if pending_question {
            let mut raw: serde_json::Value = serde_json::from_str(include_str!(
                "../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
            ))
            .unwrap();
            raw["session_id"] = "current".into();
            let mut event = parse_hook_event(HookSource::Claude, node, raw).unwrap();
            event.control_run_id = Some(run.clone());
            let ticket = HookQuestion::new(
                HookQuestionRequest {
                    run_id: run.clone(),
                    event,
                },
                std::time::Instant::now(),
            )
            .unwrap();
            runtime.accept_native_question(ticket, "p").unwrap();
        }
        runtime
            .send_node_input(node, NodeInput::Text("/resume".into()))
            .unwrap();
        fail.store(!pending_question, Ordering::SeqCst);
        let sent = runtime.send_node_input(
            node,
            NodeInput::Key(KeyInput {
                key: Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            }),
        );
        assert_eq!(
            sent.is_ok(),
            pending_question,
            "仅普通提示测试模拟Enter写失败；问答文字确实写入"
        );
        assert!(
            !runtime.observe_mobile_hook(&session_event(node, &run, "old", "resume")),
            "失败Enter或问答中的/resume不能重开退休会话"
        );
        assert_eq!(
            runtime.agent_session_id(node).as_deref(),
            Some("current"),
            "不允许旧Hook夺走当前会话身份"
        );
        assert_eq!(
            runtime.has_pending_question(node),
            pending_question,
            "回放不能关闭当前原题"
        );
    }
}
