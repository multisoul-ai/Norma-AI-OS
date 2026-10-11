use agent::card_instance::{CardInstanceAssignment, CardInstanceTracking};
use agent::workspace_dispatch::{CardExecutionFingerprint, WorkspaceCardRef, WorkspaceCardTarget};
use agent::workspace_operation::{WorkspaceOperationOrigin, WorkspaceOperationSource};
use storage::{
    NewWorkspaceOperation, WorkspaceAgentRoleStore, WorkspaceAssignmentKind,
    WorkspaceOperationAssignment, WorkspaceOperationAuthorization, WorkspaceOperationStore,
};

pub fn source() -> WorkspaceOperationSource {
    WorkspaceOperationSource {
        workspace_id: "alpha".into(),
        origin: WorkspaceOperationOrigin::Desktop {
            host_id: "host-a".into(),
        },
        operation_id: "operation-a".into(),
        message_id: "message-a".into(),
        original_text: "让 Alan 和 Bella 修复登录".into(),
    }
}
pub fn item(id: &str, area: &str, name: &str) -> WorkspaceOperationAssignment {
    WorkspaceOperationAssignment {
        assignment: CardInstanceAssignment {
            assignment_id: id.into(),
            target: WorkspaceCardTarget {
                card: WorkspaceCardRef {
                    workspace_id: "alpha".into(),
                    area_id: area.into(),
                    node_id: 1,
                    card_instance_id: format!("card-{id}"),
                },
                execution: CardExecutionFingerprint {
                    runtime_instance_id: "runtime-a".into(),
                    route_generation: 1,
                    terminal_session_id: Some(1),
                    run_id: Some("run-a".into()),
                    session_id: Some("session-a".into()),
                },
            },
            name_at_acceptance: name.into(),
            definition_id: None,
            text: "修复登录".into(),
            tracking: CardInstanceTracking::Managed,
            force_untracked: false,
        },
        kind: WorkspaceAssignmentKind::Task,
        task_ref: format!("task-{id}"),
        logical_task_ref: format!("task-{id}"),
        attempt_sequence: 1,
        project_id: "project-a".into(),
        project_name: "项目甲".into(),
        area_name: area.into(),
    }
}
pub fn setup() -> (
    tempfile::TempDir,
    WorkspaceOperationStore,
    NewWorkspaceOperation,
) {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let path = temp.path().join("app.sqlite");
    let roles = WorkspaceAgentRoleStore::new(path.clone());
    let items = vec![
        item("assignment-a", "area-a", "Alan"),
        item("assignment-b", "area-b", "Bella"),
    ];
    for item in &items {
        roles
            .create(
                &item.assignment.target.card,
                &item.assignment.name_at_acceptance,
                true,
            )
            .expect("应登记角色");
    }
    let conn = rusqlite::Connection::open(&path).expect("应构造目录fixture");
    conn.execute_batch("INSERT INTO desktop_workspaces(id,name,created_at_unix_ms,last_opened_at_unix_ms) VALUES('alpha','空间甲',1,1); INSERT INTO desktop_project_areas(id,desktop_workspace_id,project_key,root_path,display_name,added_at_unix_ms) VALUES('area-a','alpha','project-a','/fixture/project','项目甲',1),('area-b','alpha','project-a','/fixture/project','项目甲',2);").expect("应保存完整区域归属");
    (
        temp,
        WorkspaceOperationStore::new(path),
        NewWorkspaceOperation {
            source: source(),
            original_request_fingerprint: "d0".repeat(32),
            assignments: items,
            expected_role_revision: 2,
            authorization: WorkspaceOperationAuthorization::Desktop,
        },
    )
}
