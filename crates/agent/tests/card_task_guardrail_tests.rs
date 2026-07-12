mod support;

use agent::{AgentCommand, AgentEvent};
use norma::protocol::{CardCommand, CardSummary, CardSummaryKind};
use support::{response, system, text_response, tool, RecordingHost, ScriptedProvider};

/// G-1：CardTask 没有任何成功工具回执时不得把模型文本当作任务成功。
///
/// 数据构造：DirectCardTask 的 Subagent 直接返回“已完成”，没有发起工具调用；CardToolHost 因此不会产生回执。
/// 执行过程：入队任务后调用一次 `pump` 启动 Subagent，并检查公开 AgentEvent 与 host 调用记录。
/// 预期结果：任务以 TaskFailed 收口且错误指出没有成功回执；不得发出 TaskSucceeded，也不得产生卡片副作用。
#[test]
fn card_task_without_a_successful_tool_receipt_cannot_succeed() {
    let mut system = system(
        ScriptedProvider::default(),
        ScriptedProvider::new([text_response("已经完成全部操作")]),
        RecordingHost::default(),
    );
    let queued = system
        .submit(AgentCommand::DirectCardTask("创建一个终端".to_string()))
        .expect("DirectCardTask 应成功入队");
    let task_id = support::task_id_from(&queued);

    let terminal = system.pump();

    assert!(
        terminal.iter().any(|event| matches!(
            event,
            AgentEvent::TaskFailed { task_id: id, reason }
                if *id == task_id && reason.contains("no successful card-tool receipt")
        )),
        "没有成功工具回执时必须以可审计的失败原因结束"
    );
    assert!(
        !terminal.iter().any(
            |event| matches!(event, AgentEvent::TaskSucceeded { task_id: id, .. } if *id == task_id)
        ),
        "模型文字即使宣称完成，也不得在无工具回执时产生 TaskSucceeded"
    );
    assert!(
        system.card_tool_host().commands.is_empty(),
        "没有工具调用的任务不得产生卡片副作用"
    );
}

/// G-2：操作既有 `node_id` 前必须有本任务的 `list_cards` 成功回执。
///
/// 数据构造：Subagent 直接对 node_id=42 调用 `send_text`，没有先调用 `list_cards`；RecordingHost 会接受任意到达的命令。
/// 执行过程：启动任务、执行这次工具调用，再检查 host 命令与公开任务事件。
/// 预期结果：未验证目标的 send_text 被 runtime 拒绝且不触碰 host；后续无成功回执时任务失败，不能伪造成功。
#[test]
fn card_task_rejects_existing_card_operations_without_a_list_receipt() {
    let mut system = system(
        ScriptedProvider::default(),
        ScriptedProvider::new([
            response([tool(
                "send-without-list",
                "send_text",
                r#"{"node_id":42,"text":"pwd","submit":true}"#,
            )]),
            text_response("已经发送到终端"),
        ]),
        RecordingHost::default(),
    );
    let queued = system
        .submit(AgentCommand::DirectCardTask(
            "向现有终端发送 pwd".to_string(),
        ))
        .expect("DirectCardTask 应成功入队");
    let task_id = support::task_id_from(&queued);

    let _ = system.pump();
    let _ = system.pump();
    let terminal = system.pump();

    assert!(
        system.card_tool_host().commands.is_empty(),
        "未先 list_cards 验证的既有 node_id 不得抵达 CardToolHost"
    );
    assert!(
        system.journal().entries().iter().any(|entry| matches!(
            entry,
            agent::AuditEntry::ToolCall(record)
                if record.tool_call_id == "send-without-list"
                    && matches!(&record.result, Err(error) if error.contains("TargetUnverified"))
        )),
        "目标未验证必须留下包含稳定 call id 与拒绝原因的工具调用审计"
    );
    assert!(
        terminal.iter().any(|event| matches!(
            event,
            AgentEvent::TaskFailed { task_id: id, reason }
                if *id == task_id && reason.contains("no successful card-tool receipt")
        )),
        "未验证调用没有成功回执时，任务不得转为成功"
    );
}

/// G-3：CardTask 成功播报必须来自成功工具回执，且保持十字以内的简洁话术。
///
/// 数据构造：Subagent 先创建卡片并收到 node_id=1，随后返回虚假的“已发送到卡片 99”终态文本。
/// 执行过程：完成 create 工具调用后取得无工具调用的终态响应，检查公开 TaskSucceeded 摘要。
/// 预期结果：摘要为“已创建卡片”；不得保留模型虚构的卡片 99 或发送事实，且不超过 10 个字符。
#[test]
fn card_task_success_summary_is_derived_from_tool_receipts_not_model_text() {
    let mut system = system(
        ScriptedProvider::default(),
        ScriptedProvider::new([
            response([tool("create-1", "create_card", r#"{"kind":"terminal"}"#)]),
            text_response("已经发送到卡片 99"),
        ]),
        RecordingHost::default(),
    );
    let queued = system
        .submit(AgentCommand::DirectCardTask("创建一个终端".to_string()))
        .expect("DirectCardTask 应成功入队");
    let task_id = support::task_id_from(&queued);

    let _ = system.pump();
    let _ = system.pump();
    let terminal = system.pump();

    let summary = terminal.iter().find_map(|event| match event {
        AgentEvent::TaskSucceeded {
            task_id: id,
            summary,
        } if *id == task_id => Some(summary),
        _ => None,
    });
    assert_eq!(
        summary,
        Some(&"已创建卡片".to_string()),
        "创建卡片成功时，用户播报必须使用简洁且事实性的固定话术"
    );
    assert!(
        summary.is_some_and(|text| !text.contains("卡片 99") && !text.contains("发送")),
        "成功摘要不得采用模型虚构的卡片 99 或发送事实"
    );
    assert!(
        summary.is_some_and(|text| text.chars().count() <= 10),
        "成功播报必须不超过 10 个字符，实际摘要：{summary:?}"
    );
}

/// G-3b：删除与其他成功操作并存时，简洁播报必须优先说明删除副作用。
///
/// 数据构造（含关键数值的推导过程）：
///   create_card 回执 = node_id 1（RecordingHost 对首个 create 的固定回执）
///   delete_card 目标 = node_id 1（直接使用本任务 Created 回执）
///   成功工具调用总数 = 2（create + delete）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Subagent 创建卡片并收到 Created(#1)；
///   2. Subagent 删除刚创建的 #1，收到 Deleted(#1)；
///   3. Subagent 停止调用工具，运行时从两条成功回执生成终态播报。
///
/// 预期结果：
///   - 断言 A：成功播报为“已删除卡片”，优先告知用户破坏性副作用；
///   - 断言 B：播报不超过 10 个字符，继续满足简洁语音反馈限制。
#[test]
fn card_task_delete_takes_priority_in_compact_success_report() {
    let mut system = system(
        ScriptedProvider::default(),
        ScriptedProvider::new([
            response([tool("create-1", "create_card", r#"{"kind":"terminal"}"#)]),
            response([tool("delete-1", "delete_card", r#"{"node_id":1}"#)]),
            text_response("处理完毕"),
        ]),
        RecordingHost::default(),
    );
    let queued = system
        .submit(AgentCommand::DirectCardTask("创建后删除终端".to_string()))
        .expect("DirectCardTask 应成功入队");
    let task_id = support::task_id_from(&queued);

    let mut observed = Vec::new();
    for _ in 0..5 {
        observed.extend(system.pump());
    }
    let summary = observed.iter().find_map(|event| match event {
        AgentEvent::TaskSucceeded {
            task_id: id,
            summary,
        } if *id == task_id => Some(summary),
        _ => None,
    });

    assert_eq!(
        summary,
        Some(&"已删除卡片".to_string()),
        "删除成功时，成功播报必须优先说明该破坏性副作用"
    );
    assert!(
        summary.is_some_and(|text| text.chars().count() <= 10),
        "删除成功播报必须不超过 10 个字符，实际摘要：{summary:?}"
    );
}

/// G-4：成功 `list_cards` 回执中的既有节点可在同一任务内继续操作。
///
/// 数据构造：list 回执只包含 node_id=42 的终端卡片；Subagent 随后向同一节点提交 `pwd`。
/// 执行过程：按 list、send_text、终态文本的三轮响应推进任务，并检查 host 调用顺序与最终摘要。
/// 预期结果：list 后 send_text 可抵达 host；成功播报仅表达已提交内容，不暴露内部查证步骤。
#[test]
fn card_task_allows_an_existing_card_after_its_list_receipt() {
    let mut host = RecordingHost::default();
    host.listed_cards = vec![CardSummary {
        node_id: 42,
        title: "shell".to_string(),
        alias: None,
        kind: CardSummaryKind::Terminal,
        selected: false,
        focused: false,
    }];
    let mut system = system(
        ScriptedProvider::default(),
        ScriptedProvider::new([
            response([tool("list-1", "list_cards", "{}")]),
            response([tool(
                "send-42",
                "send_text",
                r#"{"node_id":42,"text":"pwd","submit":true}"#,
            )]),
            text_response("已发送"),
        ]),
        host,
    );
    let queued = system
        .submit(AgentCommand::DirectCardTask(
            "向现有终端发送 pwd".to_string(),
        ))
        .expect("DirectCardTask 应成功入队");
    let task_id = support::task_id_from(&queued);

    let mut observed = Vec::new();
    for _ in 0..5 {
        observed.extend(system.pump());
    }

    assert_eq!(
        system.card_tool_host().commands.len(),
        2,
        "list 回执验证 node_id=42 后，list 与 send_text 都应各执行一次"
    );
    assert_eq!(
        system.card_tool_host().commands[0].command,
        CardCommand::List,
        "既有卡片操作前的第一条副作用必须是 list"
    );
    assert_eq!(
        system.card_tool_host().commands[1].command,
        CardCommand::SendText {
            node_id: 42,
            text: "pwd".to_string(),
            submit: true,
        },
        "验证后的既有卡片必须保留默认 submit=true 的 send_text 语义"
    );
    assert!(
        observed.contains(&AgentEvent::TaskSucceeded {
            task_id,
            summary: "已提交内容".to_string(),
        }),
        "成功事件必须只用简洁话术表达真实的用户可见副作用"
    );
}

/// G-5：已发生且未恢复的工具失败必须阻止 TaskSucceeded，即使此前已有成功副作用。
///
/// 数据构造：先 create 成功得到 node_id=1，再尝试向未验证 node_id=42 发送文本并被 runtime 拒绝，模型随后停止调用工具。
/// 执行过程：依次推进 create、失败 send_text 与终态文本三轮响应，检查最终事件与实际 host 调用。
/// 预期结果：任务因未恢复的 TargetUnverified 失败；不得产生 TaskSucceeded；已完成的 create 副作用仍如实保留。
#[test]
fn card_task_with_an_unrecovered_tool_failure_cannot_succeed() {
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

    assert_eq!(
        system.card_tool_host().commands.len(),
        1,
        "create 成功后，未验证 node_id=42 的 send_text 不得触碰 host"
    );
    assert!(
        observed.iter().any(|event| matches!(
            event,
            AgentEvent::TaskFailed { task_id: id, reason }
                if *id == task_id
                    && reason.contains("unresolved card-tool failure")
                    && reason.contains("TargetUnverified")
        )),
        "存在未恢复的 TargetUnverified 时必须以失败终态如实报告"
    );
    assert!(
        !observed.iter().any(
            |event| matches!(event, AgentEvent::TaskSucceeded { task_id: id, .. } if *id == task_id)
        ),
        "未恢复的工具失败不得被模型“全部完成”文本覆盖为成功"
    );
}

/// G-6：无关工具成功不能掩盖失败操作；`list_cards` 后必须重试原操作才算恢复。
///
/// 数据构造：create 成功后 send_text(node_id=42) 因未验证失败；随后 list_cards 成功但列表为空，模型直接结束。
/// 执行过程：推进 create、失败 send、成功 list 和终态文本四轮响应，观察最终任务状态。
/// 预期结果：成功 list 只提供查证，不能清除未重试的 send 失败；任务保持失败而非成功。
#[test]
fn card_task_list_does_not_resolve_a_different_failed_operation() {
    let mut system = system(
        ScriptedProvider::default(),
        ScriptedProvider::new([
            response([tool("create-1", "create_card", r#"{"kind":"terminal"}"#)]),
            response([tool(
                "send-unverified",
                "send_text",
                r#"{"node_id":42,"text":"pwd","submit":true}"#,
            )]),
            response([tool("list-after-error", "list_cards", "{}")]),
            text_response("已经处理完毕"),
        ]),
        RecordingHost::default(),
    );
    let queued = system
        .submit(AgentCommand::DirectCardTask(
            "创建后向既有卡片发送文本".to_string(),
        ))
        .expect("DirectCardTask 应成功入队");
    let task_id = support::task_id_from(&queued);

    let mut observed = Vec::new();
    for _ in 0..7 {
        observed.extend(system.pump());
    }

    assert!(
        observed.iter().any(|event| matches!(
            event,
            AgentEvent::TaskFailed { task_id: id, reason }
                if *id == task_id && reason.contains("TargetUnverified")
        )),
        "list_cards 未重试 send_text 时，TargetUnverified 失败必须保持未恢复"
    );
    assert!(
        !observed.iter().any(
            |event| matches!(event, AgentEvent::TaskSucceeded { task_id: id, .. } if *id == task_id)
        ),
        "无关 list 成功不得把失败 send_text 变为 TaskSucceeded"
    );
}

/// G-7：失败操作在 list 查证后重试相同命令时可以恢复并成功完成。
///
/// 数据构造：首次 send_text(node_id=42) 因未验证失败；之后 list 回执包含 #42，第二次相同 send_text 成功。
/// 执行过程：推进 create、失败 send、list、重试 send 与终态文本五轮响应，检查 host 调用和最终事件。
/// 预期结果：重试 send_text 抵达 host；任务成功，摘要只列三个成功回执而不把第一次失败说成已执行。
#[test]
fn card_task_can_recover_by_retrying_the_same_command_after_list() {
    let mut host = RecordingHost::default();
    host.listed_cards = vec![CardSummary {
        node_id: 42,
        title: "existing shell".to_string(),
        alias: None,
        kind: CardSummaryKind::Terminal,
        selected: false,
        focused: false,
    }];
    let send_arguments = r#"{"node_id":42,"text":"pwd","submit":true}"#;
    let mut system = system(
        ScriptedProvider::default(),
        ScriptedProvider::new([
            response([tool("create-1", "create_card", r#"{"kind":"terminal"}"#)]),
            response([tool("send-before-list", "send_text", send_arguments)]),
            response([tool("list-42", "list_cards", "{}")]),
            response([tool("send-after-list", "send_text", send_arguments)]),
            text_response("处理完毕"),
        ]),
        host,
    );
    let queued = system
        .submit(AgentCommand::DirectCardTask(
            "创建后向既有卡片发送 pwd".to_string(),
        ))
        .expect("DirectCardTask 应成功入队");
    let task_id = support::task_id_from(&queued);

    let mut observed = Vec::new();
    for _ in 0..9 {
        observed.extend(system.pump());
    }

    assert_eq!(
        system.card_tool_host().commands.len(),
        3,
        "create、验证 list 和恢复后的 send_text 应各执行一次；首次失败 send 不得抵达 host"
    );
    assert_eq!(
        system.card_tool_host().commands[2].command,
        CardCommand::SendText {
            node_id: 42,
            text: "pwd".to_string(),
            submit: true,
        },
        "list 验证后必须允许重试同一 send_text 命令"
    );
    assert!(
        observed.contains(&AgentEvent::TaskSucceeded {
            task_id,
            summary: "已创建并提交".to_string(),
        }),
        "相同失败命令重试成功后任务应使用真实成功回执收口"
    );
}
