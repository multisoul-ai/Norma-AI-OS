pub mod support;

use agent::{AgentCommand, AgentEvent};
use norma::protocol::{CardSummary, CardSummaryKind};
use support::{response, system, text_response, tool, RecordingHost, ScriptedProvider};

/// F-1：解析失败的工具调用不能被后续无关成功调用掩盖。
///
/// 数据构造：Subagent 先调用未注册的 `rename_card`，随后 create_card 成功并返回终态文本。
/// 执行过程：推进非法调用、创建和终态三轮响应，检查公开 AgentEvent。
/// 预期结果：非法调用没有可验证的恢复命令，任务必须失败；后续 create 成功不得转为 TaskSucceeded。
#[test]
fn card_task_invalid_tool_call_remains_unresolved_after_an_unrelated_success() {
    let mut system = system(
        ScriptedProvider::default(),
        ScriptedProvider::new([
            response([tool(
                "invalid-rename",
                "rename_card",
                r#"{"node_id":42,"title":"new"}"#,
            )]),
            response([tool(
                "create-after-invalid",
                "create_card",
                r#"{"kind":"terminal"}"#,
            )]),
            text_response("全部完成"),
        ]),
        RecordingHost::default(),
    );
    let queued = system
        .submit(AgentCommand::DirectCardTask("改名并创建终端".to_string()))
        .expect("DirectCardTask 应成功入队");
    let task_id = support::task_id_from(&queued);

    let mut observed = Vec::new();
    for _ in 0..5 {
        observed.extend(system.pump());
    }

    assert!(
        observed.iter().any(|event| matches!(
            event,
            AgentEvent::TaskFailed { task_id: id, reason }
                if *id == task_id
                    && reason.contains("unresolved card-tool failures")
                    && reason.contains("rename_card")
        )),
        "无法解析的工具调用必须作为未恢复失败保留到任务终态"
    );
    assert!(
        !observed.iter().any(
            |event| matches!(event, AgentEvent::TaskSucceeded { task_id: id, .. } if *id == task_id)
        ),
        "后续 create 成功不得掩盖先前未恢复的 rename_card 失败"
    );
}

/// F-2：多个失败操作必须分别重试；只恢复其中一个不能完成任务。
///
/// 数据构造：create 成功后 send_text(#42) 与 focus_card(#43) 均因未 list 失败；list 回执含 #42/#43，随后只重试 send_text(#42)。
/// 执行过程：推进五次工具响应和终态文本，检查 host 仅收到 create、list、重试 send 三次调用。
/// 预期结果：focus_card(#43) 仍是未恢复失败，任务失败且不得因 #42 的恢复转为成功。
#[test]
fn card_task_requires_every_failed_operation_to_be_recovered() {
    let host = RecordingHost {
        listed_cards: vec![
            CardSummary {
                node_id: 42,
                title: "terminal-a".to_string(),
                alias: None,
                kind: CardSummaryKind::Terminal,
                selected: false,
                focused: false,
            },
            CardSummary {
                node_id: 43,
                title: "terminal-b".to_string(),
                alias: None,
                kind: CardSummaryKind::Terminal,
                selected: false,
                focused: false,
            },
        ],
        ..RecordingHost::default()
    };
    let mut system = system(
        ScriptedProvider::default(),
        ScriptedProvider::new([
            response([tool("create-1", "create_card", r#"{"kind":"terminal"}"#)]),
            response([tool(
                "send-42-before-list",
                "send_text",
                r#"{"node_id":42,"text":"pwd","submit":true}"#,
            )]),
            response([tool(
                "focus-43-before-list",
                "focus_card",
                r#"{"node_id":43}"#,
            )]),
            response([tool("list-42-and-43", "list_cards", "{}")]),
            response([tool(
                "send-42-after-list",
                "send_text",
                r#"{"node_id":42,"text":"pwd","submit":true}"#,
            )]),
            text_response("全部完成"),
        ]),
        host,
    );
    let queued = system
        .submit(AgentCommand::DirectCardTask(
            "创建并处理两张既有卡片".to_string(),
        ))
        .expect("DirectCardTask 应成功入队");
    let task_id = support::task_id_from(&queued);

    let mut observed = Vec::new();
    for _ in 0..11 {
        observed.extend(system.pump());
    }

    assert_eq!(
        system.card_tool_host().commands.len(),
        3,
        "只有 create、list 和重试 send_text 应抵达 host；两次未验证操作都不得抵达"
    );
    assert!(
        observed.iter().any(|event| matches!(
            event,
            AgentEvent::TaskFailed { task_id: id, reason }
                if *id == task_id
                    && reason.contains("TargetUnverified")
                    && reason.contains("node_id=43")
        )),
        "只恢复 send_text(#42) 时，focus_card(#43) 的失败必须仍出现在终态原因中"
    );
    assert!(
        !observed.iter().any(
            |event| matches!(event, AgentEvent::TaskSucceeded { task_id: id, .. } if *id == task_id)
        ),
        "多个失败中仍有一个未恢复时任务不得成功"
    );
}

/// F-3：失败终态必须同时陈述可验证的已完成操作和阻止完成的事实。
///
/// 数据构造：先创建卡片 #1 成功，再对未验证的既有卡片 #42 调用 send_text 并被 runtime 拒绝，随后模型结束。
/// 执行过程：推进三轮响应至失败终态，从公开 TaskRecord 与 AgentEvent 读取摘要和错误。
/// 预期结果：失败摘要与失败事件都必须包含 create 回执中的 #1 和 TargetUnverified；原始错误仍单独保存，不能把部分副作用隐去。
#[test]
fn card_task_failure_summary_preserves_successful_receipts() {
    let mut system = system(
        ScriptedProvider::default(),
        ScriptedProvider::new([
            response([tool("create-1", "create_card", r#"{"kind":"terminal"}"#)]),
            response([tool(
                "send-unverified",
                "send_text",
                r#"{"node_id":42,"text":"pwd","submit":true}"#,
            )]),
            text_response("全部完成"),
        ]),
        RecordingHost::default(),
    );
    let queued = system
        .submit(AgentCommand::DirectCardTask(
            "创建终端后向另一张卡片发送 pwd".to_string(),
        ))
        .expect("DirectCardTask 应成功入队");
    let task_id = support::task_id_from(&queued);

    let mut observed = Vec::new();
    for _ in 0..5 {
        observed.extend(system.pump());
    }

    let record = system
        .task_record(task_id)
        .expect("失败任务必须仍可通过公开 task_record 查询");
    let summary = record
        .summary
        .as_deref()
        .expect("失败任务必须保存面向用户的事实摘要");
    assert!(
        summary.contains("创建卡片 #1") && summary.contains("TargetUnverified"),
        "失败摘要必须同时保留真实 create 回执和阻止完成的 TargetUnverified 原因"
    );
    assert!(
        record
            .error
            .as_deref()
            .is_some_and(|error| error.contains("TargetUnverified")),
        "TaskRecord.error 必须保留可机器检索的原始失败原因"
    );
    assert!(
        observed.iter().any(|event| matches!(
            event,
            AgentEvent::TaskFailed { task_id: id, reason }
                if *id == task_id
                    && reason.contains("创建卡片 #1")
                    && reason.contains("TargetUnverified")
        )),
        "TaskFailed 事件必须携带与任务记录一致的事实性失败摘要"
    );
}
