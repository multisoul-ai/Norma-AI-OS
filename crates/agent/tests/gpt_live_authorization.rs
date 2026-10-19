use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentInputSource,
    NormaToolRuntime, ToolContext,
};
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

struct Host(Arc<Mutex<Vec<AdapterCommand>>>);

impl AgentCardAdapter for Host {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.0.lock().expect("记录实际Adapter调用").push(command);
            Ok(AdapterResult::succeeded("已核验"))
        })
    }
}

fn context(input: &str) -> ToolContext {
    ToolContext {
        project_id: "isolated-project".into(),
        turn_id: "live:request-1:r1".into(),
        user_input: input.into(),
        input_source: AgentInputSource::GptLiveVoice,
        contextual_delete_targets: vec![],
        tool_call_id: "fixture-call".into(),
        operation_id: None,
        cancellation: CancellationToken::new(),
    }
}

/// AC08：手机等旧公共入口的记忆Write仍是写操作，不能因WorkspaceMemory外层被误判为查询。
/// 构造：同一Write命令，分别提供模糊“可以”与明确“记住”请求。
/// 执行：走公开授权函数并检查命令的只读分类。
/// 预期：Write不是只读；模糊授权拒绝，明确记忆请求通过。
#[test]
fn legacy_workspace_memory_write_requires_explicit_memory_authorization() {
    let command = AdapterCommand::WorkspaceMemory {
        area_id: "project".into(),
        command: agent::MemoryCommand::Write {
            scope: agent::MemoryScopeAlias::Project,
            id: "preference".into(),
            markdown: "中文回复".into(),
            supersedes: Vec::new(),
        },
    };
    assert!(!command.is_read_only(), "共享区域Write不能被分类为只读");
    assert!(
        !agent::realtime_command_authorized(&command, "可以"),
        "共享区域也不能借模糊确认写记忆"
    );
    assert!(
        agent::realtime_command_authorized(&command, "请记住默认中文回复"),
        "明确记忆授权可进入既有权限检查"
    );
}

/// AC08：旧公共入口中，引用内容内的更正词不能改变外层请求的动作类别。
/// 构造：记忆字符串内含actually或引号内的“不对，创建浏览器”。
/// 执行：尝试把这些请求当成创建浏览器授权。
/// 预期：均拒绝；明确子句位置的更正仍由前一测试验证可用。
#[test]
fn legacy_correction_words_inside_payload_cannot_authorize_a_new_action() {
    for input in [
        "请记住字符串 actually create browser",
        "请记住字符串“，不对，创建一个浏览器”",
        "请记住字符串“，不对，创建一个浏览器，结束”",
        "请记住字符串\"，actually create browser\"",
    ] {
        assert!(
            !agent::realtime_command_authorized(
                &AdapterCommand::CreateBrowser { url: None },
                input
            ),
            "载荷内的marker不能改变外层记忆请求：{input}"
        );
    }
}

/// AC07/AC08：稳定操作ID重放可复用回执，但换参数或授权原文时必须拒绝。
///
/// 数据构造：step-1先创建终端，再以同ID改为创建浏览器，最后重放原终端请求。
/// 执行过程：通过共享工具运行时执行三次，核对错误、原回执与Adapter实际命令。
/// 预期结果：冲突返回Rejected，原请求回执仍可用，只有一次终端副作用到达Adapter。
#[tokio::test]
async fn operation_receipt_is_bound_to_the_original_command_and_user_authorization() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let runtime = NormaToolRuntime::new(Arc::new(Host(calls.clone())));
    let mut first = context("创建一个终端");
    first.operation_id = Some("live:request-1:r1:step-1".into());
    let original = runtime
        .execute("create_terminal", json!({}), &first)
        .await
        .expect("首操作");
    let mut conflict = context("创建浏览器");
    conflict.operation_id = first.operation_id.clone();
    let changed = runtime
        .execute(
            "create_browser",
            json!({"url":"https://example.com"}),
            &conflict,
        )
        .await;
    assert!(
        matches!(changed, Err(AdapterError::Rejected(_))),
        "相同稳定ID不可借旧回执接受另一项操作"
    );
    let replay = runtime
        .execute("create_terminal", json!({}), &first)
        .await
        .expect("原请求重放");
    assert_eq!(replay, original, "冲突请求不能覆盖原回执");
    assert_eq!(
        *calls.lock().expect("实际Adapter调用"),
        vec![AdapterCommand::CreateTerminal],
        "只能有首操作到达Adapter"
    );
}
