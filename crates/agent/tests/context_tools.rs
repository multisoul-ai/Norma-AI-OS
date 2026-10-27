use agent::context::*;
use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, NormaToolRuntime, ToolContext,
};
use futures::future::BoxFuture;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Capture(Mutex<Vec<AdapterCommand>>);
impl AgentCardAdapter for Capture {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        self.0.lock().unwrap().push(command);
        Box::pin(async { Ok(AdapterResult::succeeded("captured")) })
    }
}

/// 场景：模型只能提交类型化候选；内部作用域检查不暴露成可调用工具。
/// 构造：真实 ToolContext、有效输入引用和一个保留原话的目标候选。
/// 执行：通过公开工具解析入口提交候选和读取引用，再尝试内部检查名称。
/// 预期：前两项准确传到宿主；内部名称与额外未知字段被拒绝。
#[tokio::test]
async fn context_tools_preserve_typed_candidates_and_hide_host_checkpoint() {
    let capture = Arc::new(Capture::default());
    let runtime = NormaToolRuntime::new(capture.clone());
    let context = ToolContext {
        project_id: "/p".into(),
        turn_id: "t".into(),
        user_input: "先补测试".into(),
        input_source: agent::AgentInputSource::Voice,
        contextual_delete_targets: vec![],
        tool_call_id: "call".into(),
        operation_id: None,
        cancellation: Default::default(),
    };
    let reference = ContextRef {
        scope: ContextScope {
            workspace_id: "w".into(),
            area_id: "a".into(),
            project: ContextProject::Personal,
            task_id: None,
            conversation_id: None,
        },
        event_id: "input".into(),
        revision: 1,
    };
    let update = ContextStateUpdate {
        input: reference.clone(),
        expected_revision: 1,
        change: ContextStateChange::Goal {
            text: "先补测试".into(),
            supersedes: None,
        },
    };
    runtime
        .execute(
            "context_update",
            serde_json::to_value(&update).unwrap(),
            &context,
        )
        .await
        .unwrap();
    runtime
        .execute(
            "context_read",
            serde_json::json!({"reference":reference}),
            &context,
        )
        .await
        .unwrap();
    let calls = capture.0.lock().unwrap().clone();
    assert_eq!(
        calls,
        vec![
            AdapterCommand::UpdateWorkingState(Box::new(update)),
            AdapterCommand::ReadSharedContext {
                reference,
                offset: 0
            }
        ],
        "模型参数不能丢失来源与版本"
    );
    assert!(
        runtime
            .execute("context_checkpoint", serde_json::json!({}), &context)
            .await
            .is_err(),
        "内部检查不可供模型调用"
    );
    assert!(
        runtime
            .execute(
                "context_read",
                serde_json::json!({"reference":null,"scope":"other"}),
                &context
            )
            .await
            .is_err(),
        "未知字段不能扩大读取范围"
    );
}
