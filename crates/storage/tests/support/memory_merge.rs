use agent::{AgentInputSource, MemoryCommand, MemoryScopeAlias, ToolContext};
use serde_json::json;
use storage::{DatabasePaths, MemoryAccess};

pub(super) fn context(turn: &str) -> ToolContext {
    ToolContext {
        project_id: String::new(),
        turn_id: turn.into(),
        user_input: "请合并重复记忆，保留简洁中文偏好及构建命令".into(),
        input_source: AgentInputSource::Other,
        contextual_delete_targets: vec![],
        tool_call_id: "merge-call".into(),
        operation_id: Some(format!("{turn}:merge-call")),
        cancellation: Default::default(),
    }
}

pub(super) fn merge() -> MemoryCommand {
    serde_json::from_value(json!({
        "action": "write", "scope": "personal", "id": "preferences",
        "markdown": "请用简洁中文交流。构建命令为 cargo build。",
        "supersedes": ["old-preferences"]
    }))
    .expect("写入协议应支持原子合并并删除重复主题")
}

pub(super) fn seed(paths: &DatabasePaths) -> MemoryAccess {
    let mut access = MemoryAccess::new(paths.clone(), None);
    access
        .settings_save(
            MemoryScopeAlias::Personal,
            agent::MemoryRevision::INITIAL,
            vec![
                (
                    "old-preferences".into(),
                    "请用中文交流。构建命令为 cargo build。".into(),
                ),
                ("preferences".into(), "请用简洁中文交流。".into()),
                ("device".into(), "使用 Mac。".into()),
            ],
            "历史重复记忆",
            "seed",
        )
        .expect("应保存两份重叠偏好及独立设备信息");
    access.activate().expect("激活当前目录");
    access
}

pub(super) fn read(access: &mut MemoryAccess, id: &str, turn: &str) {
    let output = access
        .execute(
            MemoryCommand::Read {
                scope: MemoryScopeAlias::Personal,
                id: id.into(),
                offset: 0,
            },
            &context(turn),
        )
        .expect("应读取合并涉及的主题");
    assert!(output.next_offset.is_none(), "短主题必须完整读取才能合并");
}
