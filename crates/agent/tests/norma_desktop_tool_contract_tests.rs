use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, NormaToolRuntime, ToolContext,
    WorkspaceSnapshotScope,
};
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct RecordingAdapter {
    commands: Mutex<Vec<AdapterCommand>>,
}

impl AgentCardAdapter for RecordingAdapter {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.commands
                .lock()
                .expect("recording adapter command lock should not be poisoned")
                .push(command);
            Ok(AdapterResult::accepted("task accepted").with_task_id(41))
        })
    }
}

/// TOOL-DESKTOP-1：模型快照和派活调用必须形成带稳定项目身份的 typed command。
///
/// 数据构造：工作空间 dw-42、项目 area-7、Agent 卡片 21，单条完整任务。
/// 执行过程：依次提交只读快照与派活工具，再提交一个缺少 area_id 的无效派活。
/// 预期结果：Adapter 只收到两条有效命令；无效派活在解析阶段被拒绝。
#[tokio::test]
async fn desktop_tools_bind_snapshot_scope_and_exact_task_target() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let context = ToolContext {
        project_id: String::new(),
        turn_id: "desktop-turn-1".into(),
        user_input: "看看进度，再让 Codex 检查登录方案".into(),
        input_source: agent::AgentInputSource::Voice,
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-1".into(),
        operation_id: None,
        cancellation: CancellationToken::new(),
    };

    runtime
        .execute("workspace_snapshot", json!({"scope":"workspace"}), &context)
        .await
        .expect("只读查询应形成命令");
    runtime
        .execute(
            "assign_workspace_task",
            json!({
                "workspace_id":"dw-42",
                "area_id":"area-7",
                "node_id":21,
                "task":"检查登录方案"
            }),
            &context,
        )
        .await
        .expect("稳定目标派活应形成命令");
    let invalid = runtime
        .execute(
            "assign_workspace_task",
            json!({"workspace_id":"dw-42","node_id":21,"task":"检查登录方案"}),
            &context,
        )
        .await;
    assert!(invalid.is_err(), "缺少项目 ID 的派活不得到达 App");

    let commands = adapter.commands.lock().expect("应读取工具命令");
    assert_eq!(commands.len(), 2, "无效调用不得产生第三条 Adapter 命令");
    assert_eq!(
        commands[0],
        AdapterCommand::WorkspaceSnapshot {
            scope: WorkspaceSnapshotScope::Workspace,
            area_id: None,
        },
        "进度查询必须是明确范围的只读命令"
    );
    assert_eq!(
        commands[1],
        AdapterCommand::AssignWorkspaceTask {
            workspace_id: "dw-42".into(),
            area_id: "area-7".into(),
            node_id: 21,
            task: "检查登录方案".into(),
        },
        "任务正文和三个稳定目标身份必须完整传给 App"
    );
}

/// 成果打开工具必须只引用工作空间快照给出的任务产物身份。
///
/// 数据构造：项目 area-7 的 Agent 21 在 turn-9 产出 plan.md，冻结版本为 sha-1。
/// 执行过程：提交完整成果引用，再提交缺少版本的调用。
/// 预期结果：完整引用形成一条 typed command；不完整引用在 Adapter 前被拒绝。
#[tokio::test]
async fn open_workspace_artifact_requires_exact_task_bound_version() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let context = ToolContext {
        project_id: String::new(),
        turn_id: "desktop-turn-2".into(),
        user_input: "打开 plan.md".into(),
        input_source: agent::AgentInputSource::Voice,
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-2".into(),
        operation_id: None,
        cancellation: CancellationToken::new(),
    };
    runtime
        .execute(
            "open_workspace_artifact",
            json!({
                "workspace_id":"dw-42",
                "area_id":"area-7",
                "node_id":21,
                "turn_id":"turn-9",
                "source_path":"plan.md",
                "version":"sha-1"
            }),
            &context,
        )
        .await
        .expect("完整的任务产物引用应形成命令");
    let invalid = runtime
        .execute(
            "open_workspace_artifact",
            json!({"workspace_id":"dw-42","area_id":"area-7","node_id":21,
                "turn_id":"turn-9","source_path":"plan.md"}),
            &context,
        )
        .await;
    assert!(invalid.is_err(), "缺少冻结版本的产物不得打开");
    let commands = adapter.commands.lock().expect("应读取工具命令");
    assert_eq!(commands.len(), 1, "无效调用不得到达 App");
    assert_eq!(
        commands[0],
        AdapterCommand::OpenWorkspaceArtifact {
            workspace_id: "dw-42".into(),
            area_id: "area-7".into(),
            node_id: 21,
            turn_id: "turn-9".into(),
            source_path: "plan.md".into(),
            version: "sha-1".into(),
        },
        "成果引用应完整保留任务、路径和版本身份"
    );
}

/// Agent 与待审定位共用稳定目标工具，审批必须额外绑定回合身份。
///
/// 数据构造：项目 area-7 的 Agent 21 当前回合 run-3／turn-9 等待审批。
/// 执行过程：提交普通定位和完整审批定位，再提交只带 run_id 的无效审批定位。
/// 预期结果：前两次形成 typed command；不完整审批引用在 Adapter 前被拒绝。
#[tokio::test]
async fn locate_workspace_agent_requires_complete_approval_reference() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let context = ToolContext {
        project_id: String::new(),
        turn_id: "desktop-turn-3".into(),
        user_input: "定位 Codex 的审批".into(),
        input_source: agent::AgentInputSource::Voice,
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-3".into(),
        operation_id: None,
        cancellation: CancellationToken::new(),
    };
    runtime
        .execute(
            "locate_workspace_agent",
            json!({"workspace_id":"dw-42","area_id":"area-7","node_id":21}),
            &context,
        )
        .await
        .expect("普通 Agent 定位应形成命令");
    runtime
        .execute(
            "locate_workspace_agent",
            json!({"workspace_id":"dw-42","area_id":"area-7","node_id":21,
                "approval_run_id":"run-3","approval_turn_id":"turn-9"}),
            &context,
        )
        .await
        .expect("完整审批引用应形成命令");
    let invalid = runtime
        .execute(
            "locate_workspace_agent",
            json!({"workspace_id":"dw-42","area_id":"area-7","node_id":21,
                "approval_run_id":"run-3"}),
            &context,
        )
        .await;
    assert!(invalid.is_err(), "不完整审批引用不得到达 App");
    let commands = adapter.commands.lock().expect("应读取工具命令");
    assert_eq!(commands.len(), 2, "只应收到两条完整定位命令");
    assert_eq!(
        commands[0],
        AdapterCommand::LocateWorkspaceAgent {
            workspace_id: "dw-42".into(),
            area_id: "area-7".into(),
            node_id: 21,
            approval_run_id: None,
            approval_turn_id: None,
        },
        "普通定位应只绑定卡片目标"
    );
    assert_eq!(
        commands[1],
        AdapterCommand::LocateWorkspaceAgent {
            workspace_id: "dw-42".into(),
            area_id: "area-7".into(),
            node_id: 21,
            approval_run_id: Some("run-3".into()),
            approval_turn_id: Some("turn-9".into()),
        },
        "待审定位应绑定同一回合"
    );
}
