use crate::workspace::dispatch::{
    WorkspaceDispatchEntry, WorkspaceDispatchPreparation, WorkspaceDispatchService,
};
use agent::{card_instance::CardInstanceStage, WorkspaceTaskPlan};
use norma::{workspace_dispatch::WorkspaceRoutingMode, workspace_operation::*};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::dispatch_test_support::with_workspace;

/// 场景：卡片正文已经写入后停用角色，不能在异步授权更改后继续 Enter。
/// 构造：两项目已登记普通卡，派给 Betty；100ms 的 paste-to-Enter 窗口内停用原角色。
/// 执行：共享服务接收→仅推进正文→调用生产角色停用事务→推进 Enter 并读取持久回执。
/// 预期：B 只有正文无 Enter，A 零输入；待核对且不能安全重试，重新启用不自动重放旧项。
#[test]
fn shared_service_role_disable_after_body_revokes_enter_and_never_revives_old_input() {
    with_workspace(|host, cx, paths, workspace, _, sessions, writes| {
        let service = WorkspaceDispatchService::new(paths.app_db_path().to_owned());
        let source = WorkspaceOperationSource {
            workspace_id: workspace.id.clone(),
            origin: WorkspaceOperationOrigin::Desktop {
                host_id: "host".into(),
            },
            operation_id: "disable-operation".into(),
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
            .expect("绑定原卡")
        {
            WorkspaceDispatchPreparation::Ready(p) => p,
            _ => panic!("必须先绑定原卡"),
        };
        let plan = WorkspaceTaskPlan::from_response(
            r#"{"assignments":[{"target_index":0,"text":"停用前仅写正文"}]}"#,
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
            .expect("接收");
        let card = receipt.assignments[0].assignment.target.card.clone();
        let now = Instant::now();
        sessions[1].1.update(cx, |app, _| {
            app.advance_card_instance_tasks_at(now);
            app.journal_workspace_card_instance_events();
        });
        crate::workspace::dispatch::authority::set_role_enabled(paths.app_db_path(), &card, false)
            .expect("生产停用事务");
        sessions[1].1.update(cx, |app, _| {
            app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
            app.journal_workspace_card_instance_events();
        });
        assert_eq!(
            *writes[1].lock().expect("B"),
            vec!["停用前仅写正文".as_bytes().to_vec()],
            "停用后不能发送 Enter"
        );
        assert!(writes[0].lock().expect("A").is_empty(), "非目标 A 零输入");
        let operation = storage::WorkspaceOperationStore::new(paths.app_db_path().to_owned());
        let deadline = Instant::now() + Duration::from_secs(5);
        let stored = loop {
            let stored = operation.receipt(&source).expect("读取").expect("原项");
            if stored.assignments[0].receipt.stage == CardInstanceStage::OutcomeUnknown {
                break stored;
            }
            assert!(Instant::now() < deadline, "部分写入停用应持久标记待核对");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            !stored.assignments[0]
                .receipt
                .failure
                .as_ref()
                .expect("明确失败原因")
                .retry_safe,
            "可能写入后的任务不得安全重发"
        );
        crate::workspace::dispatch::authority::set_role_enabled(paths.app_db_path(), &card, true)
            .expect("重新启用");
        sessions[1].1.update(cx, |app, _| {
            app.advance_card_instance_tasks_at(now + Duration::from_secs(1));
        });
        assert_eq!(
            writes[1].lock().expect("B").len(),
            1,
            "重新启用不能恢复旧 Enter 或重放任务"
        );
    });
}

/// 场景：模型等待期间原卡被替换，同名新卡不能接收旧计划。
/// 构造：完整空间两张卡；prepare Betty 后用空文档移除原身份，再建同名卡。
/// 执行：提交先前完整计划，通过共享服务与公开账本检查拒绝和副作用。
/// 预期：旧目标失效，整组零接收；所有已有 PTY 零输入，未重选或启动替代目标。
#[test]
fn shared_service_rejects_replaced_target_before_atomic_acceptance() {
    with_workspace(|host, cx, paths, workspace, _, sessions, writes| {
        let service = WorkspaceDispatchService::new(paths.app_db_path().to_owned());
        let source = WorkspaceOperationSource {
            workspace_id: workspace.id.clone(),
            origin: WorkspaceOperationOrigin::Desktop {
                host_id: "host".into(),
            },
            operation_id: "replaced-operation".into(),
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
            .expect("绑定原卡")
        {
            WorkspaceDispatchPreparation::Ready(p) => p,
            _ => panic!("必须先绑定原卡"),
        };
        let plan = WorkspaceTaskPlan::from_response(
            r#"{"assignments":[{"target_index":0,"text":"禁止误投的旧正文"}]}"#,
            &pending.cards(),
        )
        .expect("计划");
        sessions[1].1.update(cx, |app, _| {
            app.runtime.document = canvas::Document::new();
            app.runtime.document.add_node(
                canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
                    profile: canvas::AgentProfile::OpenCode,
                    alias: Some("Betty".into()),
                    definition_id: None,
                    mobile_control: false,
                }),
                canvas::CanvasRect::new(0., 0., 640., 400.),
            );
        });
        let capture = Arc::new(Mutex::new(None));
        let output = capture.clone();
        let task = host.update(cx, |_, cx| service.submit(pending, plan, cx));
        cx.spawn(async move |_| {
            *output.lock().expect("结果锁") = Some(task.await);
        })
        .detach();
        cx.run_until_parked();
        assert!(
            capture
                .lock()
                .expect("结果")
                .take()
                .expect("submit")
                .is_err(),
            "替换后的卡不得接收旧计划"
        );
        assert!(
            storage::WorkspaceOperationStore::new(paths.app_db_path().to_owned())
                .receipt(&source)
                .expect("公开账本")
                .is_none(),
            "预检失败整组零接收"
        );
        assert!(writes[0].lock().expect("A").is_empty(), "A 零输入");
        assert!(writes[1].lock().expect("旧 B").is_empty(), "旧 B 零输入");
        assert_eq!(
            sessions[1].1.read_with(cx, |app, _| app.runtime.live_len()),
            1,
            "不得为同名替代卡启动新 PTY"
        );
    });
}
/// 场景：正式共享服务冻结第二项目，整组持久接收后只投原卡；重复操作不重新投递。
/// 构造：一个空间、两个项目，每项目一张已登记 OpenCode 卡，独立记录 PTY。
/// 执行：在 A 展开时准备 Betty 任务→切到另一空间→submit→推进 B→重发相同来源。
/// 预期：B 正文/Enter 一次，A 零输入；原来源回执 Submitted，导航不跳回，未创建额外 PTY。
#[test]
fn shared_service_delivers_to_frozen_background_card_and_replays_only_receipts() {
    with_workspace(|host, cx, paths, workspace, other, sessions, writes| {
        let service = WorkspaceDispatchService::new(paths.app_db_path().to_owned());
        let source = WorkspaceOperationSource {
            workspace_id: workspace.id.clone(),
            origin: WorkspaceOperationOrigin::Desktop {
                host_id: "host".into(),
            },
            operation_id: "operation".into(),
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
                .expect("桌面来源"),
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
            .expect("结果锁")
            .take()
            .expect("后台 prepare")
            .expect("prepare 成功")
        {
            WorkspaceDispatchPreparation::Ready(pending) => pending,
            _ => panic!("首次应准备原目标"),
        };
        let plan = WorkspaceTaskPlan::from_response(
            r#"{"assignments":[{"target_index":0,"text":"后台原卡唯一正文"}]}"#,
            &pending.cards(),
        )
        .expect("完整计划");
        host.update(cx, |app, _| {
            app.desktop_workspace.active_id = Some(other.id.clone())
        });
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
            .expect("结果锁")
            .take()
            .expect("submit 完成")
            .expect("整组接收");
        assert_eq!(receipt.source, source, "来源不因导航改变");
        assert_eq!(receipt.assignments.len(), 1, "只接收用户点名目标");
        let now = Instant::now();
        sessions[1].1.update(cx, |app, _| {
            app.advance_card_instance_tasks_at(now);
            app.journal_workspace_card_instance_events();
            app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
            app.journal_workspace_card_instance_events();
        });
        let operation = storage::WorkspaceOperationStore::new(paths.app_db_path().to_owned());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let stored = operation.receipt(&source).expect("只读").expect("操作");
            if stored.assignments[0].receipt.stage == CardInstanceStage::Submitted {
                break;
            }
            assert!(Instant::now() < deadline, "应在后台保存 Submitted");
            std::thread::sleep(Duration::from_millis(10));
        }
        let capture = Arc::new(Mutex::new(None));
        let output = capture.clone();
        let task = host.update(cx, |_, cx| {
            service.prepare(
                WorkspaceDispatchEntry::desktop(source, WorkspaceRoutingMode::Execute, vec![])
                    .expect("重发来源"),
                cx,
            )
        });
        cx.spawn(async move |_| {
            *output.lock().expect("结果锁") = Some(task.await);
        })
        .detach();
        cx.run_until_parked();
        assert!(
            matches!(
                capture
                    .lock()
                    .expect("结果锁")
                    .take()
                    .expect("重发完成")
                    .expect("重发"),
                WorkspaceDispatchPreparation::Existing(_)
            ),
            "重发仅返回原回执，不能再次准备或安装队列"
        );
        sessions[1].1.update(cx, |app, _| {
            app.advance_card_instance_tasks_at(now + Duration::from_secs(1));
        });
        assert!(writes[0].lock().expect("A").is_empty(), "非目标 A 零输入");
        assert_eq!(
            *writes[1].lock().expect("B"),
            vec!["后台原卡唯一正文".as_bytes().to_vec(), vec![13]],
            "重复操作不能重复正文或 Enter"
        );
        assert_eq!(
            host.read_with(cx, |app, _| app.desktop_workspace.active_id.clone()),
            Some(other.id.clone()),
            "后台投递不改变用户导航"
        );
        assert_eq!(
            sessions[0].1.read_with(cx, |app, _| app.runtime.live_len()),
            1,
            "A 不启动其他 PTY"
        );
        assert_eq!(
            sessions[1].1.read_with(cx, |app, _| app.runtime.live_len()),
            1,
            "B 复用原 PTY"
        );
    });
}
