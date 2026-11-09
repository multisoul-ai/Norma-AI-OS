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
        voice_commit: None,
        contextual_delete_targets: vec![],
        tool_call_id: "call-1".into(),
        operation_id: Some("doubao:turn-1:call-1".into()),
        cancellation: Default::default(),
    }
}

/// 场景：任务正文只用模型从原话里去掉称呼后的那句话，宿主不再在后面附加整段原话。
/// 构造：原话是对 Norma 的派活指令（“那让你看一下 Grace 你给她派发个任务。看他方案有什么问题。”），
///   模型按规则只留下“看他方案有什么问题。”；五条派发/续聊/交接入口各执行一次。
/// 执行：经公开工具运行时执行，检查宿主实际收到的正文与目标身份。
/// 预期：正文与模型传入完全一致，不出现分隔线或原话副本；工具类型与目标不变。
#[tokio::test]
async fn delegation_routes_forward_task_without_appending_transcript() {
    let original = "那让你看一下 Grace 你给她派发个任务。看他方案有什么问题。";
    let task = "看他方案有什么问题。";
    for (tool, arguments) in [
        ("agent_assign", json!({"agent":"Grace","task":task})),
        ("agent_reply", json!({"agent":"Grace","message":task})),
        ("agent_create", json!({"profile":"codex","task":task})),
        (
            "agent_assign",
            json!({"workspace_id":"ws-1","area_id":"area-2","node_id":7,"task":task}),
        ),
        ("handoff_to_brain", json!({"request":task})),
    ] {
        let adapter = Arc::new(RecordingAdapter::default());
        NormaToolRuntime::new(adapter.clone())
            .execute(tool, arguments, &context(original))
            .await
            .expect("合法任务应交给宿主");
        let commands = adapter.0.lock().expect("实际交接记录");
        assert_eq!(commands.len(), 1, "{tool} 只交接一次");
        let text = match (tool, &commands[0]) {
            ("agent_assign", AdapterCommand::AssignAgentTask { agent, task }) => {
                assert_eq!(agent, "Grace", "派发目标不得变化");
                task
            }
            ("agent_reply", AdapterCommand::ReplyAgent { agent, message }) => {
                assert_eq!(agent, "Grace", "续聊目标不得变化");
                message
            }
            ("agent_create", AdapterCommand::CreateAgent { task, .. }) => {
                task.as_ref().expect("附带任务不能丢失")
            }
            ("agent_assign", AdapterCommand::AssignWorkspaceTask { task, .. }) => task,
            ("handoff_to_brain", AdapterCommand::HandoffToBrain { request }) => request,
            _ => panic!("{tool} 不得变为其他操作"),
        };
        assert_eq!(text, task, "{tool} 必须原样发送任务正文");
        assert!(!text.contains("---"), "{tool} 不得追加分隔线和原话副本");
    }
}

/// 场景：派活正文要尽量等于用户原话，只去掉对目标 Agent 的称呼和派活用语。
/// 构造：空画布与名册，使用生产公开入口生成固定系统指令。
/// 执行：检查逐字保留规则、禁止润色/扩写，以及“Daisy，你去看下…”的示例。
/// 预期：指令要求逐字保留并给出去掉称呼的示例；不再要求润色，简短播报不适用于任务正文。
#[test]
fn delegation_prompt_requires_verbatim_task_text() {
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
        prompt.contains("正文只放要做的事：去掉称呼和派活用语"),
        "派发正文只放要做的事，去掉称呼和派活用语"
    );
    assert!(!prompt.contains("润色只调整表达"), "不得再指示润色任务正文");
}

/// 场景：模型生成正文时漏掉了原话里的否定限制，宿主兜底附上原话；没漏则不附加。
/// 构造：原话“Daisy，你去看下登录页有什么问题，别改数据库，不要提交”；
///   漏限制正文“你去看下登录页有什么问题”，完整正文“你去看下登录页有什么问题，别改数据库，不要提交”，
///   以及只去掉“能不能/方不方便”询问、“不对”纠正称呼的正文，和漏掉“无需”限制的正文。
/// 执行：经公开工具运行时逐个派发，检查宿主实际收到的正文。
/// 预期：漏限制（含“无需”）时正文后附原话并提示只做自己的部分；完整保留或只去掉寒暄/询问/纠正时原样发送。
#[tokio::test]
async fn delegation_appends_transcript_only_when_constraints_dropped() {
    let original = "Daisy，你去看下登录页有什么问题，别改数据库，不要提交";
    let kept = "你去看下登录页有什么问题，别改数据库，不要提交";
    for (original, task, expected) in [
        (
            original,
            "你去看下登录页有什么问题",
            format!(
                "你去看下登录页有什么问题\n\n用户原话（核对限制，只做派给你的部分）：{original}"
            ),
        ),
        (original, kept, kept.to_string()),
        (
            "Grace 能不能帮我看下登录页",
            "帮我看下登录页",
            "帮我看下登录页".to_string(),
        ),
    ] {
        let adapter = Arc::new(RecordingAdapter::default());
        NormaToolRuntime::new(adapter.clone())
            .execute(
                "agent_assign",
                json!({"agent":"Daisy","task":task}),
                &context(original),
            )
            .await
            .expect("合法任务应交给宿主");
        let commands = adapter.0.lock().expect("实际交接记录");
        let AdapterCommand::AssignAgentTask { task: sent, .. } = &commands[0] else {
            panic!("必须仍是派发任务");
        };
        assert_eq!(sent, &expected, "正文 {task:?} 的兜底结果不符");
    }
}
