use super::test_support::with_cards;
use agent::card_instance::{CardInstanceAssignment, CardInstanceStage, CardInstanceTracking};
use agent::workspace_operation::{WorkspaceOperationOrigin, WorkspaceOperationSource};
use std::sync::{atomic::AtomicBool, Arc};
use std::time::{Duration, Instant};
use storage::{
    NewWorkspaceOperation, WorkspaceAgentRoleStore, WorkspaceAssignmentKind, WorkspaceInputPhase,
    WorkspaceOperationAssignment, WorkspaceOperationAuthorization, WorkspaceOperationStore,
};

/// 场景：生产卡片适配器仅接收持久 Armed 的原实例，帧事件在后台落账。
/// 构造：两张无定义 OpenCode 卡；登记第二卡并原子接收一项任务，首次 Armed 才投递。
/// 执行：安装持久任务，推进正文与 101ms 后 Enter，再通过公开回执等待 journal。
/// 预期：原卡收到正文及 CR，非目标零输入；正文/Enter 分列，未伪造运行、完成或 turn。
#[test]
fn durable_delivery_journals_body_and_enter_without_touching_other_cards() {
    with_cards(|app, cx, nodes, writes| {
        let temp = tempfile::tempdir().expect("隔离账本");
        let path = temp.path().join("app.sqlite");
        let prepared = app.update(cx, |app, _| {
            app.workspace_id = Some("desktop:workspace:area".into());
            let target = app
                .runtime
                .workspace_card_target(
                    "workspace",
                    "area",
                    app.workspace_dispatch_route_generation(),
                    nodes[1],
                )
                .expect("冻结真实目标");
            WorkspaceOperationAssignment {
                assignment: CardInstanceAssignment {
                    assignment_id: "durable-target".into(),
                    target,
                    name_at_acceptance: "Betty".into(),
                    definition_id: None,
                    text: "持久派工唯一正文".into(),
                    tracking: CardInstanceTracking::Untracked,
                    force_untracked: false,
                },
                kind: WorkspaceAssignmentKind::Task,
                task_ref: "task-target".into(),
                logical_task_ref: "task-target".into(),
                attempt_sequence: 1,
                project_id: "project".into(),
                project_name: "项目".into(),
                area_name: "画布".into(),
            }
        });
        WorkspaceAgentRoleStore::new(path.clone())
            .create(&prepared.assignment.target.card, "Betty", true)
            .expect("登记原实例");
        let connection = rusqlite::Connection::open(&path).expect("目录 fixture");
        connection.execute_batch("INSERT INTO desktop_workspaces(id,name,created_at_unix_ms,last_opened_at_unix_ms) VALUES('workspace','空间',1,1); INSERT INTO desktop_project_areas(id,desktop_workspace_id,project_key,root_path,display_name,added_at_unix_ms) VALUES('area','workspace','project','/fixture/project','项目',1);").expect("持久项目归属");
        drop(connection);
        let store = WorkspaceOperationStore::new(path.clone());
        let source = WorkspaceOperationSource {
            workspace_id: "workspace".into(),
            origin: WorkspaceOperationOrigin::Desktop {
                host_id: "host".into(),
            },
            operation_id: "operation".into(),
            message_id: "message".into(),
            original_text: "让 Betty 修复登录".into(),
        };
        let accepted = store
            .accept(&NewWorkspaceOperation {
                source: source.clone(),
                original_request_fingerprint: "ab".repeat(32),
                assignments: vec![prepared],
                expected_role_revision: 1,
                authorization: WorkspaceOperationAuthorization::Desktop,
            })
            .expect("持久原子接收");
        let mut async_cx = cx.to_async();
        let journal = cx
            .executor()
            .block_test(crate::workspace::dispatch::journal::shared_journal(
                &path,
                &mut async_cx,
            ))
            .expect("后台 journal");
        let gate = Arc::new(AtomicBool::new(true));
        let queued = accepted.receipt.assignments[0].clone();
        assert!(
            app.update(cx, |app, _| app.accept_persisted_card_instance_assignment(
                &queued,
                gate.clone(),
                journal.clone()
            ))
            .is_err(),
            "未 Armed 不能装入输入队列"
        );
        assert!(
            store.arm_input("durable-target").expect("持久 Armed"),
            "首次 arm 获得投递权"
        );
        let armed = store
            .assignment("durable-target")
            .expect("读账本")
            .expect("原项");
        app.update(cx, |app, _| {
            app.accept_persisted_card_instance_assignment(&armed, gate, journal)
        })
        .expect("安装 Armed 原项");
        let now = Instant::now();
        app.update(cx, |app, _| {
            app.advance_card_instance_tasks_at(now);
            let events = app.take_card_instance_events();
            assert!(
                events
                    .iter()
                    .any(|event| event.evidence == "draft_pasted_enter_pending"),
                "公开事件消费者应能看见正文证据"
            );
            app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
            let events = app.take_card_instance_events();
            assert!(
                events
                    .iter()
                    .any(|event| event.evidence == "enter_confirmed"),
                "公开事件读取不能偷走持久写回证据"
            );
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let persisted = loop {
            let receipt = store
                .receipt(&source)
                .expect("严格只读回执")
                .expect("持久操作");
            if receipt.assignments[0].input_phase == WorkspaceInputPhase::EnterConfirmed {
                break receipt;
            }
            assert!(
                Instant::now() < deadline,
                "journal 应保存正文和 Enter，当前 {:?}",
                receipt.assignments[0]
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(
            persisted.assignments[0].receipt.stage,
            CardInstanceStage::Submitted,
            "发送不等于运行或完成"
        );
        assert_eq!(
            persisted.assignments[0].receipt.bound_turn, None,
            "没有真实 Hook 不伪造 turn"
        );
        let events = store.events("workspace", 0, 32).expect("公开事件读取");
        assert!(
            events
                .iter()
                .any(|e| e.evidence == "draft_pasted_enter_pending"),
            "必须单独记录正文阶段"
        );
        assert!(
            events.iter().any(|e| e.evidence == "enter_confirmed"),
            "必须单独记录 Enter 阶段"
        );
        assert!(writes[0].lock().expect("非目标").is_empty(), "非目标零输入");
        assert_eq!(
            *writes[1].lock().expect("原卡"),
            vec!["持久派工唯一正文".as_bytes().to_vec(), vec![13]],
            "原卡仅有正文与 CR(0x0d)"
        );
    });
}
