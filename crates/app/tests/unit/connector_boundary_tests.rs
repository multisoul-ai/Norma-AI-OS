use super::test_support::Fixture;
use serde_json::json;
/// 场景：用户在已有卡片输入未提交草稿；连接器不能覆盖或借allow_queue绕过。
/// 执行：桌面输入→MCP派工；预期input_occupied，草稿保留且无任务。
#[test]
fn connector_preserves_user_input_draft() {
    let mut f = Fixture::new();
    let card = f.create("card", 0, None);
    let identity = f
        .repo
        .signer
        .verify(card["card_id"].as_str().expect("卡片"))
        .expect("身份");
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            app.runtime
                .send_node_input(
                    identity.node_id,
                    runtime::NodeInput::Paste("human draft".into()),
                )
                .expect("桌面用户输入");
        })
    });
    assert_eq!(f.call("dispatch_task",json!({"operation_id":"occupied","card_id":card["card_id"],"card_revision":card["card_revision"],"task":"不能覆盖","allow_queue":true}))["error"]["code"],"input_occupied","用户输入明确拒绝");
    f.test.update(|cx| {
        assert!(
            f.sessions[0]
                .read(cx)
                .agent_task_coordinator
                .as_ref()
                .is_none_or(|c| c.tasks().is_empty()),
            "占用前零派工"
        )
    });
}

/// 场景：每次调用仅一条审计，任务只保存哈希和前80字符，内部拒绝不回传原文。
/// 执行：长正文派工、重放、错误映射；预期：审计数量准确，正文尾部与内部原文不泄漏。
#[test]
fn connector_audit_is_once_per_call_and_redacts_internal_rejection() {
    let mut f = Fixture::new();
    let card = f.create("card", 0, None);
    let text = format!("{}private-tail", "前".repeat(80));
    let args = json!({"operation_id":"dispatch","card_id":card["card_id"],"card_revision":card["card_revision"],"task":text});
    f.call("dispatch_task", args.clone());
    f.call("dispatch_task", args);
    let conn = rusqlite::Connection::open(&f.repo.database).expect("隔离审计库");
    let records: Vec<(String, String, String)> = conn
        .prepare(
            "SELECT tool,parameter_summary,operation_id FROM connector_audit ORDER BY sequence",
        )
        .expect("审计")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("审计查询")
        .collect::<Result<_, _>>()
        .expect("结果");
    assert_eq!(records.len(), 3, "建卡、派工、重放各一条");
    assert_eq!(records[0].0, "create_card", "建卡单独审计");
    for record in &records[1..] {
        let summary: serde_json::Value = serde_json::from_str(&record.1).expect("结构化摘要");
        assert_eq!(summary["source"], "connector", "明确来源");
        assert_eq!(
            summary["body_prefix"]
                .as_str()
                .expect("正文摘要")
                .chars()
                .count(),
            80,
            "80字符边界"
        );
        assert_eq!(
            summary["body_hash"].as_str().expect("正文哈希").len(),
            64,
            "SHA256十六进制"
        );
        assert!(!record.1.contains("private-tail"), "不存全文尾部");
        assert_eq!(record.2, "dispatch", "原operation_id落库");
    }
    let error = agent::AdapterError::Rejected("private internal /path/to/file".into());
    let code = super::execution::error_code(&error, false);
    assert_eq!(code, "agent_rejected", "未识别拒绝使用稳定码");
    assert!(
        !norma::connector::tool_error(code)
            .to_string()
            .contains("private internal"),
        "原文不回MCP"
    );
}

/// 场景：任务等待审批时取消；只执行目标中断，审批未被allow/deny。
/// 构造：一项已提交任务，注入WaitingApproval；执行取消后模拟Provider终止。
/// 预期：回执cancel_requested，运行状态由Provider确认；审批关闭实际CLI验证留给needs-Mac。
#[test]
fn connector_cancel_waiting_approval_does_not_decide_it() {
    let mut f = Fixture::new();
    let created = f.create("task", 0, Some("审批任务"));
    let task_ref = created["task_id"].as_str().expect("任务").to_owned();
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            let task = app
                .agent_task_coordinator
                .as_ref()
                .expect("协调器")
                .task_by_ref(&task_ref)
                .expect("目标");
            app.pending_agent_submissions.remove_task(task.task_id);
            app.agent_task_coordinator
                .as_mut()
                .expect("协调器")
                .mark_task_submitted(task.task_id);
            app.runtime
                .apply_agent_task_update(runtime::AgentTaskUpdate {
                    node_id: task.target_node_id,
                    profile: canvas::AgentProfile::Codex,
                    provider_event: "PermissionRequest".into(),
                    to: runtime::AgentTaskState::WaitingApproval {
                        tool_name: Some("Bash".into()),
                    },
                    task_title: None,
                    session_id: None,
                });
        })
    });
    let result = f.call(
        "cancel_task",
        json!({"operation_id":"cancel","task_id":task_ref}),
    );
    assert_eq!(
        result["state"], "cancel_requested",
        "中断请求不伪造Provider已关闭"
    );
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            let task = app
                .agent_task_coordinator
                .as_ref()
                .expect("协调器")
                .task_by_ref(&task_ref)
                .expect("任务");
            assert_eq!(task.state, "cancel_requested", "仅修改任务取消状态");
            assert!(
                matches!(
                    app.runtime.agent_task_state(task.target_node_id),
                    Some(runtime::AgentTaskState::WaitingApproval { .. })
                ),
                "没有替用户设置审批决定"
            );
            app.agent_task_coordinator
                .as_mut()
                .expect("协调器")
                .finish_current_with_state(task.definition_id, "cancelled");
        })
    });
    assert_eq!(
        f.call(
            "cancel_task",
            json!({"operation_id":"closed","task_id":task_ref})
        )["terminal_state"],
        "cancelled",
        "Provider确认后明确终态"
    );
}

/// 场景：App在预留建卡之后退出，原记录无完成结果；重开查询禁止自动重放。
/// 构造：真实MobileCommandStore中accepted连接器行，临时项目无卡片。
/// 执行：重复MCP建卡、只读核对；预期：outcome_unknown、时间不变、零卡片。
#[test]
fn connector_unknown_creation_receipt_never_replays_side_effects() {
    let mut f = Fixture::new();
    let input = json!({"operation_id":"crashed","workspace_id":f.workspace,"area_id":f.areas[0],"profile":"codex"});
    let request =
        norma::connector::WriteRequest::parse("create_card", input.clone(), &f.repo.signer)
            .expect("合法建卡");
    let reserved = storage::NewMobileCommand {
        device_id: format!("connector:{}", f.identity.client_epoch()),
        operation_id: "crashed".into(),
        workspace_id: f.workspace.clone(),
        route_id: f.areas[0].clone(),
        owner_id: "grokbot".into(),
        request_hash: request.fingerprint(),
    };
    let at = f.auth.now();
    storage::MobileCommandStore::new(f.repo.database.clone())
        .reserve_connector(&reserved, at)
        .expect("模拟崩溃前落盘");
    for _ in 0..2 {
        let result = f.call("create_card", input.clone());
        assert_eq!(result["state"], "outcome_unknown", "未知不能重放副作用");
        assert_eq!(result["created_at"], at, "原预留时刻");
        assert_eq!(result["replayed"], true, "只读取回执");
    }
    let progress = f.call("get_task_progress", json!({"operation_id":"crashed"}));
    assert_eq!(progress["state"], "outcome_unknown", "原操作可只读核对");
    assert_eq!(
        progress["created_at"],
        serde_json::Value::Null,
        "未知任务时间不能用预留时间冒充"
    );
    f.test.update(|cx| {
        assert_eq!(
            f.sessions[0].read(cx).runtime.document.nodes().len(),
            0,
            "始终零卡片"
        )
    });
}

/// 场景：同卡第二任务已交给CLI队列，现有Adapter返回AlreadySubmitted类拒绝。
/// 执行：两次真实派工、标记第二项submitted、取消；预期agent_rejected且原文仅审计。
#[test]
fn connector_cancel_submitted_queue_redacts_adapter_rejection() {
    let mut f = Fixture::new();
    let card = f.create("first", 0, Some("first task"));
    let queued = f.call("dispatch_task",json!({"operation_id":"second","card_id":card["card_id"],"card_revision":card["card_revision"],"task":"second task"}));
    let task_ref = queued["task_id"].as_str().expect("排队任务").to_owned();
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            let task = app
                .agent_task_coordinator
                .as_ref()
                .expect("协调器")
                .task_by_ref(&task_ref)
                .expect("排队任务");
            app.agent_task_coordinator
                .as_mut()
                .expect("协调器")
                .mark_task_submitted(task.task_id);
        })
    });
    let result = f.call(
        "cancel_task",
        json!({"operation_id":"cancel-submitted","task_id":task_ref}),
    );
    assert_eq!(
        result["error"]["code"], "agent_rejected",
        "未结构化AlreadySubmitted不猜成功"
    );
    assert!(!result.to_string().contains("CLI"), "内部拒绝原文不回传");
    let conn = rusqlite::Connection::open(&f.repo.database).expect("隔离审计");
    let summary: String = conn
        .query_row(
            "SELECT parameter_summary FROM connector_audit WHERE operation_id='cancel-submitted'",
            [],
            |r| r.get(0),
        )
        .expect("取消审计");
    assert!(summary.contains("CLI"), "原拒绝仅留在审计证据");
}
