use crate::app::MultisoulApp;
use agent::context::{ContextProject, ContextScope};
use norma::agent_hook::{parse_hook_event, HookQuestion, HookQuestionRequest, HookSource};
use norma::protocol::QuestionAnswer;
use std::time::Instant;

pub(crate) fn make_question_from_raw(
    app: &mut MultisoulApp,
    cx: &mut gpui::Context<MultisoulApp>,
    root: &std::path::Path,
    raw: serde_json::Value,
) -> (
    norma::protocol::QuestionRef,
    HookQuestion,
    Vec<QuestionAnswer>,
) {
    app.enter_canvas_for_test(root, cx);
    app.shared_context_scope = Some(ContextScope {
        workspace_id: "w".into(),
        area_id: "a".into(),
        project: ContextProject::Project {
            project_id: "project".into(),
            worktree: root.to_string_lossy().into_owned(),
        },
        task_id: None,
        conversation_id: None,
    });
    app.runtime
        .set_agent_hook_config(Some(runtime::AgentHookRuntimeConfig {
            endpoint: "http://127.0.0.1:49152/agent-hooks".into(),
            token: "test".into(),
            workspace_id: "w".into(),
            workspace_route_id: "route".into(),
            norma_socket: "/tmp/question-test.sock".into(),
        }));
    let node = app
        .runtime
        .spawn_agent(
            canvas::AgentProfile::Claude,
            canvas::CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .unwrap();
    let run = app.runtime.mobile_agent_run_id(node).unwrap().to_owned();
    let mut event = parse_hook_event(HookSource::Claude, node, raw).unwrap();
    event.control_run_id = Some(run.clone());
    app.runtime
        .apply_agent_task_update(runtime::AgentTaskUpdate {
            node_id: node,
            profile: canvas::AgentProfile::Claude,
            provider_event: "SessionStart".into(),
            to: runtime::AgentTaskState::Thinking,
            task_title: None,
            session_id: event.session_id.clone(),
        });
    let ticket =
        HookQuestion::new(HookQuestionRequest { run_id: run, event }, Instant::now()).unwrap();
    let target = app
        .runtime
        .accept_native_question(ticket.clone(), "project")
        .unwrap();
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }];
    (target, ticket, answers)
}
