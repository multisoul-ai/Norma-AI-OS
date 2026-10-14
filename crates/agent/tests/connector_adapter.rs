use agent::{AdapterCommand, AgentInputSource};
use norma::protocol::AgentProfile;

/// 场景：连接器只允许四种写命令；构造合法建卡、精确派工及禁止能力。
/// 执行：逐项检查来源边界；预期：白名单允许，终端、聚焦、删除拒绝。
#[test]
fn connector_source_has_four_command_allowlist() {
    for command in [
        AdapterCommand::CreateAgent {
            profile: AgentProfile::Codex,
            task: None,
        },
        AdapterCommand::CancelAgentTask {
            task_id: 1,
            expected_task_ref: None,
        },
        AdapterCommand::AssignAgentTask {
            agent: "display".into(),
            task: "work".into(),
            node_id: Some(7),
            expected_revision: Some("r".into()),
            target_identity: None,
            allow_queue: true,
        },
        AdapterCommand::ReplyAgent {
            agent: "display".into(),
            message: "hello".into(),
            node_id: Some(7),
            expected_revision: Some("r".into()),
            target_identity: None,
        },
    ] {
        assert!(
            command.allowed_from(AgentInputSource::Connector),
            "四种映射必须允许"
        );
    }
    for command in [
        AdapterCommand::CreateTerminal,
        AdapterCommand::FocusCard { node_id: 7 },
        AdapterCommand::DeleteCanvasCard { node_id: 7 },
        AdapterCommand::CreateAgent {
            profile: AgentProfile::Grok,
            task: None,
        },
    ] {
        assert!(
            !command.allowed_from(AgentInputSource::Connector),
            "连接器不能借内部Adapter扩大能力"
        );
        assert!(
            command.allowed_from(AgentInputSource::Other),
            "既有来源保持行为"
        );
    }
}
