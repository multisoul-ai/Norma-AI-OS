use crate::workspace::dispatch::{
    WorkspaceDispatchEntry, WorkspaceDispatchPreparation, WorkspaceDispatchService,
};
use agent::WorkspaceTaskPlan;
use norma::{workspace_dispatch::WorkspaceRoutingMode, workspace_operation::*};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::dispatch_test_support::with_workspace;

/// 场景：冷启动 Claude 的 SessionStart 晚于 PTY 启动，不能提前冻结无会话指纹。
/// 构造：原 Betty 改为冷 Claude，启用 Hook 环境，录制 PTY；首个 Hook 尚未送达。
/// 执行：prepare/submit 推进到等待就绪，再通过真实 Runtime Hook seam 发送 SessionStart。
/// 预期：Hook 前零接收/零输入；Hook 后以原 run 和 session 接收并提交，A 零输入。
#[test]
fn shared_service_waits_for_cold_hook_session_before_first_fingerprint() {
    for saved in [false, true] {
        with_workspace(|host, cx, paths, workspace, _, sessions, writes| {
            let card = sessions[1].1.update(cx, |app, _| {
                let mut cold = runtime::Runtime::new_for_workspace(&sessions[1].0.root);
                cold.restore_canvas_snapshot(app.runtime.document.to_snapshot());
                let node = cold.document.nodes()[0].id;
                if let canvas::CanvasNodeKind::Agent(spec) =
                    &mut cold.document.node_mut(node).expect("原卡").kind
                {
                    spec.profile = canvas::AgentProfile::Claude;
                    spec.alias = Some("Betty".into());
                }
                if saved {
                    cold.hydrate_agent_session_bindings(&[storage::NodeSession {
                        node_id: node,
                        kind: storage::NodeSessionKind::Agent,
                        cwd: None,
                        session_id: Some("saved-session".into()),
                    }]);
                }
                cold.set_agent_hook_config(Some(runtime::AgentHookRuntimeConfig {
                    endpoint: "http://127.0.0.1:1/agent-hooks".into(),
                    token: "test".into(),
                    workspace_id: workspace.id.clone(),
                    workspace_route_id: "cold-route".into(),
                    norma_socket: "/tmp/test.sock".into(),
                }));
                let record = writes[1].clone();
                cold.set_pty_factory(Box::new(move |_, _, _, tx| {
                    Ok(Box::new(terminal::TestPtyBackend::recording(
                        tx,
                        record.clone(),
                    )))
                }));
                app.runtime = cold;
                app.persistence
                    .project_store()
                    .expect("区域 DB")
                    .canvas_store()
                    .save_snapshot(&app.runtime.document.to_snapshot())
                    .expect("持久冷卡");
                app.workspace_card_ref(node).expect("原冷卡")
            });
            let hook_paths = hook_paths(paths.app_db_path());
            norma::agent_hook::install_hook_config(
                norma::agent_hook::HookTarget::Claude,
                std::path::Path::new(norma::agent_hook::AGENT_HOOK_BINARY_NAME),
                &hook_paths,
            )
            .expect("隔离安装 Hook");
            assert!(
                norma::agent_hook::agent_hooks_installed(
                    norma::agent_hook::HookTarget::Claude,
                    &hook_paths
                ),
                "隔离 fixture 必须真的声明已安装 Hook"
            );
            let service = WorkspaceDispatchService::new(paths.app_db_path().to_owned())
                .with_hook_paths(hook_paths);

            let pending = super::dispatch_test_support::prepare_betty(
                &host,
                cx,
                &service,
                &workspace.id,
                "cold-hook",
            );
            let plan = WorkspaceTaskPlan::from_response(
                r#"{"assignments":[{"target_index":0,"text":"Hook 就绪后的任务"}]}"#,
                &pending.cards(),
            )
            .expect("完整计划");
            let capture = Arc::new(Mutex::new(None));
            let output = capture.clone();
            let task = host.update(cx, |_, cx| service.submit(pending, plan, cx));
            cx.spawn(async move |_| {
                *output.lock().expect("结果") = Some(task.await);
            })
            .detach();
            cx.run_until_parked();
            assert!(
                capture.lock().expect("结果").is_none(),
                "首个 Hook 前不能接收无 session 指纹"
            );
            assert!(
                writes[1].lock().expect("B").is_empty(),
                "等待 Hook 不发送正文"
            );
            let run = sessions[1].1.update(cx, |app, _| {
                let run = app
                    .runtime
                    .mobile_agent_run_id(card.node_id)
                    .expect("冷启动 run")
                    .to_owned();
                let event = norma::agent_hook::parse_hook_event(
                norma::agent_hook::HookSource::Claude,
                card.node_id,
                serde_json::json!({"hook_event_name":"SessionStart", "session_id":"cold-session"}),
            )
            .expect("可信 Hook");
                assert!(app.runtime.observe_mobile_hook(&event), "原卡接收启动 Hook");
                assert!(
                    crate::settings::agent_hooks::apply_agent_hook_event(&mut app.runtime, event),
                    "生产状态更新标记已观察首 Hook"
                );
                let observed_run = app
                    .runtime
                    .mobile_agent_run_id(card.node_id)
                    .expect("已观察 run")
                    .to_owned();
                if !saved {
                    assert_eq!(observed_run, run, "新建会话不改变启动 run");
                }
                observed_run
            });
            cx.executor().advance_clock(Duration::from_millis(50));
            cx.run_until_parked();
            let receipt = capture
                .lock()
                .expect("结果")
                .take()
                .expect("Hook 后结束")
                .expect("首次就绪指纹应接收");
            let target = &receipt.assignments[0].assignment.target;
            assert_eq!(target.card, card, "没有换卡");
            assert_eq!(
                target.execution.run_id.as_deref(),
                Some(run.as_str()),
                "保留原启动 run"
            );
            assert_eq!(
                target.execution.session_id.as_deref(),
                Some("cold-session"),
                "首次捕获包含已就绪会话"
            );
            assert_eq!(
                receipt.assignments[0].assignment.tracking,
                agent::card_instance::CardInstanceTracking::Managed,
                "有真实会话才可跟踪"
            );
            let now = Instant::now();
            sessions[1].1.update(cx, |app, _| {
                app.advance_card_instance_tasks_at(now);
                app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
                app.journal_workspace_card_instance_events();
            });
            assert!(writes[0].lock().expect("A").is_empty(), "非目标零输入");
            assert_eq!(
                *writes[1].lock().expect("B"),
                vec!["Hook 就绪后的任务".as_bytes().to_vec(), vec![13]],
                "就绪后原卡仅提交一次"
            );
        });
    }
}

/// 场景：目标为未启动 PTY 的原卡，同区域另有冷卡，不得为派工启动全部卡片。
/// 构造：A 保持 live；B 恢复原 Document 再新增 Clara，两张卡均冷，记录 launch/输入。
/// 执行：公开 prepare 不启动任何卡，完整计划 submit 仅准备 Betty，再推进 PTY 输入。
/// 预期：原 Betty 新建唯一 PTY、提交一次；Clara 无 PTY、A 零输入，保留持久目标身份。
#[test]
fn shared_service_prepares_only_the_original_cold_card() {
    with_workspace(|host, cx, paths, workspace, _, sessions, writes| {
        let launched = Arc::new(Mutex::new(0usize));
        let record = writes[1].clone();
        let launches = launched.clone();
        let (card, other_node) = sessions[1].1.update(cx, |app, _| {
            let mut cold = runtime::Runtime::new_for_workspace(&sessions[1].0.root);
            cold.restore_canvas_snapshot(app.runtime.document.to_snapshot());
            let node = cold.document.nodes()[0].id;
            if let canvas::CanvasNodeKind::Agent(spec) =
                &mut cold.document.node_mut(node).expect("原卡").kind
            {
                spec.alias = Some("Betty".into());
            }
            let other = cold.document.add_node(
                canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
                    profile: canvas::AgentProfile::OpenCode,
                    alias: Some("Clara".into()),
                    definition_id: None,
                    mobile_control: false,
                }),
                canvas::CanvasRect::new(700., 0., 640., 400.),
            );
            cold.set_agent_hook_config(Some(runtime::AgentHookRuntimeConfig {
                endpoint: "http://127.0.0.1:1/agent-hooks".into(),
                token: "test".into(),
                workspace_id: workspace.id.clone(),
                workspace_route_id: "cold-route".into(),
                norma_socket: "/tmp/test.sock".into(),
            }));
            cold.set_pty_factory(Box::new(move |_, _, _, tx| {
                *launches.lock().expect("启动记录") += 1;
                Ok(Box::new(terminal::TestPtyBackend::recording(
                    tx,
                    record.clone(),
                )))
            }));
            app.runtime = cold;
            let other_card = app.workspace_card_ref(other).expect("另一冷卡身份");
            storage::WorkspaceAgentRoleStore::new(paths.app_db_path().to_owned())
                .create(&other_card, "Clara", true)
                .expect("登记另一冷卡");
            app.persistence
                .project_store()
                .expect("区域 DB")
                .canvas_store()
                .save_snapshot(&app.runtime.document.to_snapshot())
                .expect("两卡持久快照");
            (app.workspace_card_ref(node).expect("原冷卡"), other)
        });
        let service = WorkspaceDispatchService::new(paths.app_db_path().to_owned())
            .with_hook_paths(hook_paths(paths.app_db_path()));
        let source = WorkspaceOperationSource {
            workspace_id: workspace.id.clone(),
            origin: WorkspaceOperationOrigin::Desktop {
                host_id: "host".into(),
            },
            operation_id: "cold-operation".into(),
            message_id: "message".into(),
            original_text: "让 Betty 修复登录".into(),
        };
        let capture = Arc::new(Mutex::new(None));
        let output = capture.clone();
        let task = host.update(cx, |_, cx| {
            service.prepare(
                WorkspaceDispatchEntry::desktop(
                    source.clone(),
                    WorkspaceRoutingMode::Execute,
                    vec![],
                )
                .expect("来源"),
                cx,
            )
        });
        cx.spawn(async move |_| {
            *output.lock().expect("结果锁") = Some(task.await);
        })
        .detach();
        cx.run_until_parked();
        let pending = match capture
            .lock()
            .expect("结果")
            .take()
            .expect("prepare")
            .expect("绑定冷卡")
        {
            WorkspaceDispatchPreparation::Ready(p) => p,
            WorkspaceDispatchPreparation::NeedsTarget(reason) => {
                panic!("必须绑定持久原卡: {reason:?}")
            }
            _ => panic!("不得复用其他操作"),
        };
        assert_eq!(pending.cards(), vec![card.clone()], "冷准备仍保留原实例");
        assert_eq!(
            *launched.lock().expect("启动计数"),
            0,
            "prepare 不得启动 PTY"
        );
        let plan = WorkspaceTaskPlan::from_response(
            r#"{"assignments":[{"target_index":0,"text":"冷卡原任务"}]}"#,
            &pending.cards(),
        )
        .expect("计划");
        let capture = Arc::new(Mutex::new(None));
        let output = capture.clone();
        let task = host.update(cx, |_, cx| service.submit(pending, plan, cx));
        cx.spawn(async move |_| {
            *output.lock().expect("结果锁") = Some(task.await);
        })
        .detach();
        cx.run_until_parked();
        let receipt = capture
            .lock()
            .expect("结果")
            .take()
            .expect("submit")
            .expect("应异步准备原冷卡并接收");
        assert_eq!(
            receipt.assignments[0].assignment.target.card, card,
            "只捕获冷卡首次真实执行身份，不换卡"
        );
        assert!(
            receipt.assignments[0]
                .assignment
                .target
                .execution
                .terminal_session_id
                .is_some(),
            "Armed 前必须有真实 PTY 身份"
        );
        assert_eq!(*launched.lock().expect("启动计数"), 1, "只启动 Betty 原卡");
        sessions[1].1.read_with(cx, |app, _| {
            assert_eq!(app.runtime.live_len(), 1, "同区域另一冷卡不启动");
            assert!(
                app.runtime.node_run_state(other_node).is_none(),
                "Clara 保持冷状态"
            );
        });
        let now = Instant::now();
        sessions[1].1.update(cx, |app, _| {
            app.advance_card_instance_tasks_at(now);
            app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
            app.journal_workspace_card_instance_events();
        });
        assert!(writes[0].lock().expect("A").is_empty(), "A 零输入");
        assert_eq!(
            *writes[1].lock().expect("B"),
            vec!["冷卡原任务".as_bytes().to_vec(), vec![13]],
            "原冷卡仅提交一次"
        );
    });
}

fn hook_paths(database: &std::path::Path) -> norma::agent_hook::HookInstallPaths {
    let root = database.parent().expect("隔离目录").join("hooks");
    norma::agent_hook::HookInstallPaths {
        claude_settings: root.join("claude.json"),
        codex_hooks: root.join("codex.json"),
        cursor_hooks: root.join("cursor.json"),
        grok_hooks: root.join("grok.json"),
        opencode_plugin: root.join("opencode.ts"),
    }
}
