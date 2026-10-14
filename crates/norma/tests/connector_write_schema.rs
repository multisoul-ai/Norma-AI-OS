use norma::connector::ConnectorTools;
/// 场景：P4开放四种薄映射；执行tools/list；预期：保留三个只读工具且不暴露其它Adapter能力。
#[test]
fn p4_tools_have_only_four_mutations() {
    let tools = ConnectorTools::definitions();
    let names: Vec<_> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert_eq!(
        names,
        [
            "directory",
            "list_tasks",
            "get_task_progress",
            "dispatch_task",
            "cancel_task",
            "reply_agent",
            "create_card"
        ],
        "工具白名单"
    );
    for tool in tools {
        assert_eq!(
            tool.annotations.expect("注解").read_only_hint,
            Some(matches!(
                tool.name.as_ref(),
                "directory" | "list_tasks" | "get_task_progress"
            )),
            "写工具明确非只读"
        );
        assert_eq!(
            tool.input_schema["additionalProperties"], false,
            "所有输入拒绝未知字段"
        );
    }
}

/// 场景：签名目标、四个工具的严格参数边界；构造8000/8001字符与隐藏Adapter参数。
/// 执行：解析写请求并比较省略/显式默认的指纹；预期：只有规范输入能进入账本。
#[test]
fn write_arguments_are_bounded_normalized_and_cannot_expand_adapter_capabilities() {
    use norma::connector::{CardIdentity, CardSigner, WriteRequest};
    use serde_json::json;
    let signer = CardSigner::new(&[4; 32]);
    let target = signer.sign(&CardIdentity {
        workspace_id: "w".into(),
        area_id: "a".into(),
        canvas_id: "c".into(),
        node_id: 7,
        definition_id: 1,
        created_at: 10,
    });
    let original =
        json!({"operation_id":"op","card_id":target,"card_revision":"r","task":"x".repeat(8000)});
    let request =
        WriteRequest::parse("dispatch_task", original.clone(), &signer).expect("8000字符合法");
    let mut explicit = original.clone();
    explicit["allow_queue"] = json!(true);
    assert_eq!(
        request.fingerprint(),
        WriteRequest::parse("dispatch_task", explicit, &signer)
            .expect("显式默认")
            .fingerprint(),
        "默认值指纹一致"
    );
    let mut long = original.clone();
    long["task"] = json!("x".repeat(8001));
    assert_eq!(
        WriteRequest::parse("dispatch_task", long, &signer)
            .expect_err("长文本")
            .to_string(),
        "task_too_long",
        "超过一字符拒绝"
    );
    for field in [
        "force_untracked",
        "node_id",
        "agent",
        "workspace_id",
        "target_identity",
    ] {
        let mut extra = original.clone();
        extra[field] = json!(true);
        assert_eq!(
            WriteRequest::parse("dispatch_task", extra, &signer)
                .expect_err("未知字段")
                .to_string(),
            "invalid_argument",
            "禁止内部入口参数"
        );
    }
    for name in [
        "send_terminal_input",
        "delete_canvas_card",
        "focus_card",
        "memory",
        "create_terminal",
        "create_browser",
        "create_preview",
        "cancel_agent_reply",
        "resolve_approval",
    ] {
        assert_eq!(
            WriteRequest::parse(name, json!({}), &signer)
                .expect_err("范围外工具")
                .to_string(),
            "invalid_argument",
            "不能构造额外命令"
        );
    }
    let mut forged = original;
    forged["card_id"] = json!("c1.bad.bad");
    assert!(
        WriteRequest::parse("dispatch_task", forged, &signer).is_err(),
        "签名校验在预留前"
    );
}

/// 场景：连接器任务、上下文、回复和建卡任务包含终端控制字符。
/// 构造：中文正文夹入所有 C0/C1、DEL；执行公开请求解析。
/// 预期：进入 Adapter 前剥离控制字符，保留换行与制表符，清洗后指纹一致；空正文拒绝。
#[test]
fn connector_text_strips_terminal_controls_and_preserves_multiline() {
    use norma::connector::{CardIdentity, CardSigner, WriteRequest};
    use serde_json::json;
    let signer = CardSigner::new(&[4; 32]);
    let card = signer.sign(&CardIdentity {
        workspace_id: "w".into(),
        area_id: "a".into(),
        canvas_id: "c".into(),
        node_id: 7,
        definition_id: 1,
        created_at: 10,
    });
    let controls: String = (0u8..=159)
        .map(char::from)
        .filter(|c| c.is_control() && *c != '\n' && *c != '\t')
        .collect();
    let dirty = format!("任务{controls}\n\t说明");
    let clean = "任务\n\t说明";
    for (tool, field, mut args) in [
        (
            "dispatch_task",
            "task",
            json!({"operation_id":"op","card_id":card,"card_revision":"r"}),
        ),
        (
            "reply_agent",
            "message",
            json!({"operation_id":"op","card_id":card,"card_revision":"r"}),
        ),
        (
            "create_card",
            "task",
            json!({"operation_id":"op","workspace_id":"w","area_id":"a","profile":"codex"}),
        ),
    ] {
        args[field] = json!(dirty);
        let parsed = WriteRequest::parse(tool, args.clone(), &signer).expect("剥离而非拒绝正文");
        let received = match &parsed {
            WriteRequest::Reply(reply) => reply.message.clone(),
            _ => parsed.task_text().expect("任务正文"),
        };
        assert_eq!(received, clean, "{tool} 正文在进入 Adapter 前清洗");
        assert!(!received.contains('\u{3}'), "不得借正文发送 Ctrl+C");
        args[field] = json!(clean);
        assert_eq!(
            parsed.fingerprint(),
            WriteRequest::parse(tool, args.clone(), &signer)
                .expect("干净正文")
                .fingerprint(),
            "{tool} 指纹基于规范化正文"
        );
        args[field] = json!(controls);
        assert_eq!(
            WriteRequest::parse(tool, args, &signer)
                .expect_err("清洗后为空")
                .to_string(),
            "invalid_argument",
            "剥离后仍验证非空"
        );
    }
    let request = WriteRequest::parse("dispatch_task", json!({"operation_id":"op","card_id":card,"card_revision":"r","task":"主任务","context":dirty}), &signer).expect("上下文清洗");
    assert_eq!(
        request.task_text().as_deref(),
        Some("主任务\n\n任务\n\t说明"),
        "上下文也必须清洗"
    );
}
