use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentInputSource,
    NormaToolRuntime, ToolContext,
};
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingAdapter(Mutex<Vec<AdapterCommand>>);

impl AgentCardAdapter for RecordingAdapter {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.0.lock().expect("记录实际派发").push(command);
            Ok(AdapterResult::accepted("accepted"))
        })
    }
}

fn context(original: &str) -> ToolContext {
    ToolContext {
        project_id: "project-1".into(),
        turn_id: "doubao:turn-1".into(),
        user_input: original.into(),
        input_source: AgentInputSource::DoubaoVoice,
        contextual_delete_targets: vec![],
        tool_call_id: "call-1".into(),
        operation_id: Some("doubao:turn-1:call-1".into()),
        cancellation: Default::default(),
    }
}

/// 场景：豆包把含背景和约束的长话术缩成一句任务时，执行端仍收到全部原话。
/// 构造：原话包含原因、复现步骤、路径、否定约束和交付要求；模型只传“修复登录”。
/// 执行：经公开工具运行时派发两次相同 operation_id，检查实际 Adapter 命令。
/// 预期：保留润色正文并逐字携带原话，目标不变；重试不重复执行，不只传摘要。
#[tokio::test]
async fn assignment_preserves_verbatim_request_when_model_summarizes() {
    let original = "  Alan，请修复登录。背景：客户在弱网下会重复点击。\n复现：离线后连点两次；错误在 src/auth.rs。\n不要改 API，也不要清空本地草稿。先复现再修复，最后给出测试结果。  ";
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let context = context(original);
    for _ in 0..2 {
        let result = runtime
            .execute(
                "assign_agent_task",
                json!({"agent":"Alan","task":"修复登录"}),
                &context,
            )
            .await
            .expect("派发应成功");
        assert_eq!(result.status, "accepted", "必须保留实际宿主回执");
    }
    let commands = adapter.0.lock().expect("实际派发记录");
    assert_eq!(commands.len(), 1, "相同操作重试只能派发一次");
    let AdapterCommand::AssignAgentTask { agent, task } = &commands[0] else {
        panic!("必须保持派发命令类型");
    };
    assert_eq!(agent, "Alan", "完整原话不能改变目标 Agent");
    assert!(task.starts_with("修复登录"), "保留模型整理后的任务正文");
    assert!(task.contains(original), "派发正文必须逐字包含用户全部原话");
    assert_ne!(task, "修复登录", "不能只把压缩摘要发送给执行 Agent");
}

/// 场景：续聊、新建附带任务、跨项目派发和后台交接同样可能被模型缩写。
/// 构造：英文原话保留客户背景、否定约束和文件路径，各工具只收到一句摘要。
/// 执行：分别通过公开工具入口执行，检查宿主实际收到的正文与目标身份。
/// 预期：四条入口均完整保留原话和摘要，不改变工具类型或派发目标。
#[tokio::test]
async fn all_delegation_routes_preserve_the_original_request() {
    let original = "The customer works offline. Fix login in src/auth.rs.\nKeep drafts; do not change the API. Include the regression test results.";
    for (tool, arguments) in [
        ("reply_agent", json!({"agent":"Alan","message":"Fix login"})),
        (
            "create_agent",
            json!({"profile":"codex","task":"Fix login"}),
        ),
        (
            "assign_workspace_task",
            json!({"workspace_id":"ws-1","area_id":"area-2","node_id":7,"task":"Fix login"}),
        ),
        ("handoff_to_brain", json!({"request":"Fix login"})),
    ] {
        let adapter = Arc::new(RecordingAdapter::default());
        NormaToolRuntime::new(adapter.clone())
            .execute(tool, arguments, &context(original))
            .await
            .expect("合法任务应交给宿主");
        let commands = adapter.0.lock().expect("实际交接记录");
        assert_eq!(commands.len(), 1, "{tool} 只交接一次");
        let text = match (tool, &commands[0]) {
            ("reply_agent", AdapterCommand::ReplyAgent { agent, message }) => {
                assert_eq!(agent, "Alan", "续聊目标不得变化");
                message
            }
            ("create_agent", AdapterCommand::CreateAgent { profile, task }) => {
                assert_eq!(
                    *profile,
                    norma::protocol::AgentProfile::Codex,
                    "保留 Agent 类型"
                );
                task.as_ref().expect("附带任务不能丢失")
            }
            (
                "assign_workspace_task",
                AdapterCommand::AssignWorkspaceTask {
                    workspace_id,
                    area_id,
                    node_id,
                    task,
                },
            ) => {
                assert_eq!(workspace_id, "ws-1", "工作空间不得变化");
                assert_eq!(area_id, "area-2", "目标项目不得变化");
                assert_eq!(*node_id, 7, "目标卡片不得变化");
                task
            }
            ("handoff_to_brain", AdapterCommand::HandoffToBrain { request }) => request,
            _ => panic!("{tool} 不得变为其他操作"),
        };
        assert!(text.starts_with("Fix login"), "{tool} 保留整理后的正文");
        assert!(text.contains(original), "{tool} 必须逐字传递全部用户原话");
        assert_ne!(text, "Fix login", "{tool} 不能只发送摘要");
    }
}

/// 场景：任务润色不能沿用语音播报的简短风格，导致背景或细节被删除。
/// 构造：空画布与名册，使用生产公开入口生成固定系统指令。
/// 执行：检查完整传递规则、信息类别，以及简短播报与任务正文的适用边界。
/// 预期：要求润色不删减信息，明确覆盖背景和示例；不再指示删除用户措辞。
#[test]
fn delegation_prompt_requires_lossless_polishing() {
    let prompt = agent::norma_system_prompt(
        &agent::CurrentCanvasContext {
            project_name: "test".into(),
            workspace_root: "/project".into(),
            total_cards: 0,
            cards: vec![],
            conversation_tails: vec![],
        },
        &[],
    );
    assert!(
        prompt.contains("润色") && prompt.contains("不得删减"),
        "必须明确无损润色要求"
    );
    assert!(
        prompt.contains("背景") && prompt.contains("示例"),
        "保真要求必须覆盖背景和示例"
    );
    assert!(
        prompt.contains("简短") && prompt.contains("不适用于任务正文"),
        "播报风格不能压缩任务正文"
    );
    assert!(!prompt.contains("移除称呼"), "不得继续指示删除用户原话");
}

/// 场景：保真补充只作用于豆包的任务正文，不重复完整原话，也不凭空创建任务。
/// 构造：已经含完整原话的任务、纯建卡、打开网页，以及其他来源的派发。
/// 执行：分别经公开工具运行时执行，逐项对比完整 Adapter 命令。
/// 预期：这些命令与输入一致，不增加原话副本、不把建卡变成派发、不影响其他来源。
#[tokio::test]
async fn preservation_does_not_duplicate_or_invent_tasks() {
    let original = "Keep the offline drafts.";
    let complete = "Fix login.\nKeep the offline drafts.";
    let mut cases = vec![
        (
            AgentInputSource::DoubaoVoice,
            "assign_agent_task",
            json!({"agent":"Alan","task":complete}),
            AdapterCommand::AssignAgentTask {
                agent: "Alan".into(),
                task: complete.into(),
            },
        ),
        (
            AgentInputSource::DoubaoVoice,
            "create_agent",
            json!({"profile":"codex"}),
            AdapterCommand::CreateAgent {
                profile: norma::protocol::AgentProfile::Codex,
                task: None,
            },
        ),
        (
            AgentInputSource::DoubaoVoice,
            "create_browser",
            json!({"url":"https://example.com"}),
            AdapterCommand::CreateBrowser {
                url: Some("https://example.com".into()),
            },
        ),
    ];
    for source in [
        AgentInputSource::Other,
        AgentInputSource::Voice,
        AgentInputSource::GptLiveVoice,
        AgentInputSource::MobileExecution,
    ] {
        cases.push((
            source,
            "assign_agent_task",
            json!({"agent":"Alan","task":"Fix login"}),
            AdapterCommand::AssignAgentTask {
                agent: "Alan".into(),
                task: "Fix login".into(),
            },
        ));
    }
    for (source, tool, arguments, expected) in cases {
        let adapter = Arc::new(RecordingAdapter::default());
        let mut context = context(original);
        context.input_source = source;
        NormaToolRuntime::new(adapter.clone())
            .execute(tool, arguments, &context)
            .await
            .expect("既有合法操作应成功");
        assert_eq!(
            *adapter.0.lock().expect("实际命令"),
            vec![expected],
            "{source:?}/{tool} 不应被保真补充改写或增加操作"
        );
    }
}
