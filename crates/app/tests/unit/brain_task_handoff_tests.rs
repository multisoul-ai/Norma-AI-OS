use super::*;
use agent::context::*;
use gpui::{AppContext, TestAppContext};
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

/// 场景：提议生成后，目标卡片开始等待用户确认，不能用下一步覆盖当前问题。
/// 构造：真实 Alan 卡片、记录型 PTY、一条提议及 WaitingApproval 状态。
/// 执行：先采纳并检查拒绝原因，再把卡片更新为 Done 后重新采纳。
/// 预期：等待时明确要求先处理卡片，保留提议且不输入任务；解除后任务只派发一次。
#[test]
fn brain_proposal_waits_for_confirmation_then_dispatches() {
    let (entity, mut cx, received, _dir, _test) = fixture();
    received.lock().unwrap().clear();
    entity.update(&mut cx, |app, _| {
        let node = app
            .agent_roster_for_provider()
            .into_iter()
            .find(|entry| entry.name == "Alan")
            .unwrap()
            .active_node_id
            .unwrap();
        let id = app
            .brain
            .proposals
            .add(
                super::super::proposals::Draft {
                    agent: "Alan".into(),
                    title: "补充测试".into(),
                    task: "检查边界条件".into(),
                    why: "先补验证".into(),
                },
                std::time::SystemTime::now(),
            )
            .unwrap();
        let update = |to| runtime::AgentTaskUpdate {
            node_id: node,
            profile: canvas::AgentProfile::Codex,
            provider_event: "confirmation-test".into(),
            to,
            task_title: None,
            session_id: None,
        };
        app.runtime
            .apply_agent_task_update(update(runtime::AgentTaskState::WaitingApproval {
                tool_name: Some("AskUserQuestion".into()),
            }));
        let error = app.dispatch_brain_proposal(id).unwrap_err();
        assert_eq!(
            error,
            ui::i18n::format("chrome.brain_proposal_waiting", &[("agent", "Alan".into())]),
            "必须明确提示先处理原卡片的问题或审批"
        );
        assert_ne!(
            error,
            ui::i18n::format("chrome.brain_proposal_busy", &[("agent", "Alan".into())]),
            "等待确认不能只显示通用忙碌提示"
        );
        assert_eq!(app.brain.proposals.items().len(), 1, "待确认时必须保留提议");
        assert!(
            !received_text(&received).contains("检查边界条件"),
            "不能把任务写入问答输入"
        );
        app.runtime
            .apply_agent_task_update(update(runtime::AgentTaskState::Done));
        assert!(
            app.dispatch_brain_proposal(id).is_ok(),
            "解除确认后应能采纳"
        );
        assert!(app.brain.proposals.items().is_empty(), "成功后才消费提议");
    });
    cx.run_until_parked();
    assert_eq!(
        received_text(&received).matches("检查边界条件").count(),
        1,
        "恢复后只输入一次任务"
    );
}

/// 场景：用户点击时空闲，后台准备期间目标开始等待确认；恢复后不能留下过期错误。
/// 构造：隔离项目与概览宿主、记录型 PTY、同一条提议；首次派发前状态为 Ready。
/// 执行：启动真实异步采纳入口、切为 WaitingApproval，等待失败，再切为 Done 刷新概览。
/// 预期：等待原因由实时状态展示，不持久化为旧错误；恢复后可采纳，之前没有任务字节。
#[test]
fn brain_proposal_async_confirmation_feedback_recovers() {
    let (source, mut cx, received, dir, _test) = fixture();
    received.lock().unwrap().clear();
    let host = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            storage::DatabasePaths::isolated_test(&dir.path().join("host")),
        )
    });
    let (node, id) = source.update(&mut cx, |app, _| {
        let node = app
            .agent_roster_for_provider()
            .into_iter()
            .find(|entry| entry.name == "Alan")
            .unwrap()
            .active_node_id
            .unwrap();
        let id = app
            .brain
            .proposals
            .add(
                super::super::proposals::Draft {
                    agent: "Alan".into(),
                    title: "补充测试".into(),
                    task: "检查边界条件".into(),
                    why: "先补验证".into(),
                },
                std::time::SystemTime::now(),
            )
            .unwrap();
        (node, id)
    });
    host.update(&mut cx, |app, cx| {
        app.desktop_workspace.active_id = Some("w".into());
        app.desktop_workspace
            .sessions
            .insert(("w".into(), "a".into()), source.clone());
        app.dispatch_overview_proposal("a", id, cx).detach();
    });
    let update = |to| runtime::AgentTaskUpdate {
        node_id: node,
        profile: canvas::AgentProfile::Codex,
        provider_event: "confirmation-test".into(),
        to,
        task_title: None,
        session_id: None,
    };
    source.update(&mut cx, |app, _| {
        app.runtime
            .apply_agent_task_update(update(runtime::AgentTaskState::WaitingApproval {
                tool_name: None,
            }));
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if host.read_with(&cx, |app, _| app.desktop_workspace.error.is_some()) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "异步派发必须返回明确失败"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    host.read_with(&cx, |app, _| {
        assert!(
            matches!(
                app.brain_overview().proposals[0].block,
                Some(super::super::proposals::ProposalBlock::WaitingConfirmation)
            ),
            "概览必须展示当前阻塞"
        );
        assert!(
            app.brain_proposal_feedback("a", id).is_none(),
            "不能把可恢复的等待状态保存成永久错误"
        );
    });
    source.update(&mut cx, |app, _| {
        app.runtime
            .apply_agent_task_update(update(runtime::AgentTaskState::Done));
    });
    host.update(&mut cx, |app, cx| {
        app.refresh_brain_overview(cx);
        assert!(
            app.brain_overview().proposals[0].block.is_none(),
            "恢复后可采纳"
        );
        assert!(
            app.brain_proposal_feedback("a", id).is_none(),
            "不能留下要求先回答的旧提示"
        );
    });
    assert!(
        !received_text(&received).contains("检查边界条件"),
        "等待期间不得写入任务"
    );
}
