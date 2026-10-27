use super::*;
use agent::context::*;
use gpui::TestAppContext;
use std::sync::{Arc, Mutex};

struct RecordPty(Arc<Mutex<Vec<u8>>>);
impl terminal::PtyBackend for RecordPty {
    fn write(&mut self, bytes: &[u8]) -> Result<(), terminal::PtyError> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(())
    }
    fn resize(&mut self, _: u16, _: u16) -> Result<(), terminal::PtyError> {
        Ok(())
    }
    fn kill(&mut self) {}
}

/// 构造一个已保存“只修改前端”约束、带就绪 Alan 卡片的 App，返回卡片实际收到的字节记录。
fn fixture() -> (
    gpui::Entity<MultisoulApp>,
    gpui::VisualTestContext,
    Arc<Mutex<Vec<u8>>>,
    tempfile::TempDir,
    TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let received = Arc::new(Mutex::new(Vec::new()));
    let captured = received.clone();
    let mut test = TestAppContext::single();
    let (entity, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(move |_, _, _, _| Ok(Box::new(RecordPty(captured.clone())))),
            paths,
        )
    });
    let service = entity.update(cx, |app, cx| {
        app.enter_canvas_for_test(dir.path(), cx);
        let (_, status) = std::sync::mpsc::channel();
        app.brain.host = Some(super::super::BrainHost {
            service: None,
            status,
            token: "handoff-token".into(),
        });
        let mut settings = config::BrainSettings::default();
        settings.autonomy.assign_followups = true;
        app.brain.settings = Some(settings);
        app.shared_context_service().unwrap()
    });
    entity.update_in(cx, |app, window, _| {
        let created = crate::app::execute_agent_card_command(
            app,
            window,
            norma::protocol::CardCommand::CreateAgent {
                name: Some("Alan".into()),
                profile: norma::protocol::AgentProfile::Codex,
                activate: true,
            },
        )
        .unwrap();
        let norma::protocol::CardCommandResult::AgentCreated {
            node_id: Some(node),
            ..
        } = created
        else {
            panic!("必须返回卡片")
        };
        app.pending_agent_submissions.mark_ready(node);
    });
    crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(Some("limit".into()), |access| {
            let input = access.record_input("limit", ContextOrigin::Voice, "只修改前端")?;
            access.update(
                "limit",
                &input.reference,
                &ContextStateUpdate {
                    input: input.reference.clone(),
                    expected_revision: input.reference.revision,
                    change: ContextStateChange::Constraint {
                        key: "frontend".into(),
                        text: "只修改前端".into(),
                        supersedes: None,
                        enforcement: ConstraintEnforcement::Communicated,
                    },
                },
            )
        }))
        .unwrap();
    (entity, cx.clone(), received, dir, test)
}

fn received_text(received: &Arc<Mutex<Vec<u8>>>) -> String {
    String::from_utf8_lossy(&received.lock().unwrap()).into_owned()
}

/// 场景：幕僚按自主权限自行派发后续任务，用户此前已说“只修改前端”。
/// 构造：隔离项目、记录型 PTY、就绪 Alan 卡片，以及已保存的 communicated 约束。
/// 执行：用有效幕僚令牌调用生产 execute_brain_command(Assign)。
/// 预期：卡片实际收到的字节同时含原任务、“只修改前端”约束；任务只提交一次。
#[test]
fn brain_autonomous_assign_carries_communicated_constraint() {
    let (entity, mut cx, received, _dir, _test) = fixture();
    let result = entity.update_in(&mut cx, |app, window, cx| {
        app.execute_brain_command(
            "handoff-token",
            norma::protocol::BrainOp::Assign {
                agent: "Alan".into(),
                task: "检查边界条件".into(),
            },
            Some(window),
            cx,
        )
    });
    assert!(result.is_ok(), "有权限的幕僚派发应被接受：{result:?}");
    cx.run_until_parked();
    let sent = received_text(&received);
    assert!(
        sent.contains("检查边界条件") && sent.contains("只修改前端"),
        "自主派发的任务必须携带当前约束：{sent}"
    );
}

/// 场景：用户在概览采纳幕僚提议，任务文本是提议原文，约束同样应送达。
/// 构造：同上，并加入一条指向 Alan 的待采纳提议。
/// 执行：调用生产 dispatch_brain_proposal。
/// 预期：卡片实际收到原提议任务及“只修改前端”约束，提议被消费。
#[test]
fn brain_proposal_adoption_carries_communicated_constraint() {
    let (entity, mut cx, received, _dir, _test) = fixture();
    let result = entity.update(&mut cx, |app, _| {
        let id = app
            .brain
            .proposals
            .add(
                super::super::proposals::Draft {
                    agent: "Alan".into(),
                    title: "检查边界".into(),
                    task: "检查边界条件".into(),
                    why: "先补验证".into(),
                },
                std::time::SystemTime::now(),
            )
            .unwrap();
        app.dispatch_brain_proposal(id)
    });
    assert!(result.is_ok(), "采纳应被接受：{result:?}");
    cx.run_until_parked();
    let sent = received_text(&received);
    assert!(
        sent.contains("检查边界条件") && sent.contains("只修改前端"),
        "采纳的提议任务必须携带当前约束：{sent}"
    );
}
