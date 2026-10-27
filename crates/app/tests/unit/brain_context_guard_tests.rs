use super::*;
use agent::context::*;
use gpui::TestAppContext;
use norma::protocol::{BrainDecision, BrainOp};

fn install_restriction(service: &crate::app::shared_context::SharedContext, approval: bool) {
    crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(Some("restriction".into()), move |access| {
            let (text, operation) = if approval {
                ("不要批准操作", ContextOperation::ApproveOperation)
            } else {
                ("不要派发任务", ContextOperation::DispatchTask)
            };
            let input = access.record_input("t1", ContextOrigin::Voice, text)?;
            access.update(
                "u1",
                &input.reference,
                &ContextStateUpdate {
                    input: input.reference.clone(),
                    expected_revision: input.reference.revision,
                    change: ContextStateChange::Constraint {
                        key: "pause".into(),
                        text: text.into(),
                        supersedes: None,
                        enforcement: ConstraintEnforcement::Enforced {
                            operation,
                            target: None,
                        },
                    },
                },
            )
        }))
        .unwrap();
}

/// 场景：用户点击旧提议的派发按钮时，语音已将当前任务暂停。
/// 构造：隔离库、已保存派发限制、一条尚未派发的提议。
/// 执行：调用生产 dispatch_brain_proposal 入口。
/// 预期：以共享限制拒绝，提议保留；用户确认旧提议不能隐式撤销后续限制。
#[test]
fn brain_context_guard_applies_to_user_confirmed_proposals() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let store = storage::AppStore::initialize(&paths).unwrap();
    let mut test = TestAppContext::single();
    let (entity, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let service = entity.update(cx, |app, _| app.shared_context_service().unwrap());
    install_restriction(&service, false);
    entity.update(cx, |app, _| {
        let id = app
            .brain
            .proposals
            .add(
                super::super::proposals::Draft {
                    agent: "Alan".into(),
                    title: "待执行".into(),
                    task: "修复问题".into(),
                    why: "用户目标".into(),
                },
                std::time::SystemTime::now(),
            )
            .unwrap();
        let error = app.dispatch_brain_proposal(id).unwrap_err();
        assert!(
            error.contains("context_operation_restricted"),
            "手动提议入口也须守住限制，实际为 {error}"
        );
        assert_eq!(app.brain.proposals.items().len(), 1, "被拒绝的提议仍应保留");
    });
}

/// 场景：语音明确暂停派发或批准后，后台幕僚不能绕过共享限制。
/// 构造：独立临时库、有效幕僚令牌及自主权；审批目标仍在 tracked 中。
/// 执行：先调用无限制入口，再保存语音限制并通过同一正式命令入口重试。
/// 预期：无限制时进入原派发/审批分支；限制后精确拒绝，拒绝审批与升级仍可进行。
#[test]
fn brain_context_guard_checks_assign_and_allow_without_blocking_deny_or_escalate() {
    for approval in [false, true] {
        let mut test = TestAppContext::single();
        let dir = tempfile::tempdir().unwrap();
        let (entity, cx) = test.add_window_view(|_, cx| {
            MultisoulApp::new_with_pty_factory(
                cx,
                Box::new(|_, _, _, _| {
                    Err(terminal::PtyError::Io(std::io::Error::other("not needed")))
                }),
            )
        });
        let service = entity.update(cx, |app, cx| {
            app.database_paths = storage::DatabasePaths::isolated_test(dir.path());
            app.enter_canvas_for_test(dir.path(), cx);
            let (_, status) = std::sync::mpsc::channel();
            app.brain.host = Some(super::super::BrainHost {
                service: None,
                status,
                token: "test-token".into(),
            });
            let mut settings = config::BrainSettings::default();
            settings.autonomy.assign_followups = true;
            app.brain.settings = Some(settings);
            app.brain.tracked.insert(
                "approval-1".into(),
                super::super::Tracked {
                    target: norma::mobile::ApprovalTarget {
                        node_id: 7,
                        profile: "Codex".into(),
                        run_id: "r".into(),
                        session_id: "s".into(),
                        turn_id: "t".into(),
                        request_id: "a".into(),
                    },
                    agent: "Alan".into(),
                    subject: brain::risk::ApprovalSubject {
                        tool_name: "Bash".into(),
                        command: Some("cargo test".into()),
                        file_path: None,
                        project_root: dir.path().to_path_buf(),
                    },
                    escalated: false,
                    since: std::time::Instant::now(),
                    reminded: false,
                },
            );
            app.shared_context_service().unwrap()
        });
        crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(service.snapshot())
            .unwrap();
        let command = if approval {
            BrainOp::Decide {
                approval_id: "approval-1".into(),
                decision: BrainDecision::Allow,
                reason: "tests".into(),
            }
        } else {
            BrainOp::Assign {
                agent: "nonexistent-agent".into(),
                task: "check".into(),
            }
        };
        let before = entity
            .update_in(cx, |app, window, cx| {
                app.execute_brain_command("test-token", command.clone(), Some(window), cx)
            })
            .unwrap_err()
            .to_string();
        assert!(
            !before.contains("context_"),
            "无限制状态必须进入原始执行分支：{before}"
        );
        install_restriction(&service, approval);
        entity.update_in(cx, |app, window, cx| {
            let error = app
                .execute_brain_command("test-token", command, Some(window), cx)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("context_operation_restricted"),
                "幕僚入口必须使用最新共享限制：{error}"
            );
            assert!(
                app.brain.tracked.contains_key("approval-1"),
                "被阻止的放行不得移除待审批项"
            );
            if approval {
                let denied = app
                    .execute_brain_command(
                        "test-token",
                        BrainOp::Decide {
                            approval_id: "approval-1".into(),
                            decision: BrainDecision::Deny,
                            reason: "deny".into(),
                        },
                        None,
                        cx,
                    )
                    .unwrap_err()
                    .to_string();
                assert!(
                    denied.contains("approval_closed"),
                    "拒绝应到达原 runtime 分支：{denied}"
                );
                let escalation = app.execute_brain_command(
                    "test-token",
                    BrainOp::Decide {
                        approval_id: "approval-1".into(),
                        decision: BrainDecision::Escalate,
                        reason: "ask".into(),
                    },
                    None,
                    cx,
                );
                assert!(
                    escalation.is_ok(),
                    "暂停放行不能阻止转交用户：{escalation:?}"
                );
                assert!(
                    app.brain.tracked["approval-1"].escalated,
                    "转交必须实际更新状态"
                );
            }
        });
    }
}
