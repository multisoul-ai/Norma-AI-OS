use norma::protocol::{
    validate_decision_copy, validate_proposal_copy, BrainDecision, BrainOp, DecisionCopyIssue,
};

/// 场景：简短决策按 Unicode 字符计数，50 字符可用，51 字符被拒绝。
/// 构造：49 个汉字加英文标点共 50 字符、51 个汉字，以及中英数字空格混排。
/// 执行：调用共享校验，检查临界值、字段和具体计数。
/// 预期：合法文字原样通过；超长返回结构化错误，不能按 UTF-8 字节截断。
#[test]
fn decision_copy_counts_characters_instead_of_bytes() {
    for text in [
        "字".repeat(49),
        format!("{}!", "字".repeat(49)),
        "先检查 Agent 2；尚未证实。".into(),
    ] {
        assert!(
            validate_decision_copy("reason", &text).is_ok(),
            "合法决策应通过：{text}"
        );
    }
    let error = validate_decision_copy("reason", &"字".repeat(51)).unwrap_err();
    assert_eq!(error.issue, DecisionCopyIssue::TooLong, "超长必须单独报告");
    assert_eq!(error.actual, 51, "51 个汉字应按 51 字符而不是 153 字节计数");
    assert_eq!(error.limit, 50, "所有入口使用同一 50 字符上限");
    assert_eq!(error.field, "reason", "错误必须指出超长字段");
}

/// 场景：下一步标题和理由共享预算，完整任务不被短文案限制。
/// 构造：24 字标题、25/26 字理由，加一个分隔字符分别为 50/51 字符。
/// 执行：校验组合文案，再校验携带多行长任务的 Propose。
/// 预期：合计 50 通过，51 拒绝；task 原文完整保留，不截断不改写。
#[test]
fn proposal_title_and_reason_share_one_budget() {
    let title = "字".repeat(24);
    let why = "因".repeat(25);
    assert!(
        validate_proposal_copy(&title, &why).is_ok(),
        "24 + 1 + 25 = 50 应通过"
    );
    let error = validate_proposal_copy(&title, &"因".repeat(26)).unwrap_err();
    assert_eq!(error.actual, 51, "必须计算标题和理由的合计");
    assert_eq!(error.field, "title+why", "组合超长应指向两个字段");
    let task = format!(
        "目标：保留原始指令\n- {}\n验收：cargo test",
        "详细步骤".repeat(100)
    );
    let op = BrainOp::Propose {
        agent: "Henry".into(),
        title,
        task: task.clone(),
        why,
    };
    assert!(
        op.validate_decision_copy().is_ok(),
        "完整 task 不属于 50 字符摘要"
    );
    assert!(
        matches!(op, BrainOp::Propose { task: actual, .. } if actual == task),
        "校验不得改写任务"
    );
}

/// 场景：决策必须是明确的一行文字，不能用空白或换行绕过约束。
/// 构造：空串、纯空格、LF/CR 多行，以及带首尾空格的有效理由。
/// 执行：校验文字格式并检查具体错误类型。
/// 预期：空白与换行分别拒绝；首尾空白不计入预算。
#[test]
fn decision_copy_rejects_blank_and_multiline_text() {
    for text in ["", " \t "] {
        assert_eq!(
            validate_decision_copy("reason", text).unwrap_err().issue,
            DecisionCopyIssue::Empty,
            "空白不是决策"
        );
    }
    for text in [
        "允许\n但不要推送",
        "允许\r需用户确认",
        "允许\u{000b}不得推送",
        "允许\u{000c}不得推送",
        "允许\u{0085}不得推送",
    ] {
        assert_eq!(
            validate_decision_copy("reason", text).unwrap_err().issue,
            DecisionCopyIssue::Multiline,
            "多行应重写为完整短句"
        );
    }
    assert!(
        validate_decision_copy("reason", &format!(" {} ", "字".repeat(50))).is_ok(),
        "首尾空格不占摘要预算"
    );
}

/// 场景：审批和提议使用同一规则，报告和完整回答不被误伤。
/// 构造：带 51 字符理由的审批，长报告，以及过长的 CLI 提议。
/// 执行：通过协议直接校验和 CLI 解析两条路径。
/// 预期：审批与 CLI 超长摘要被拒；普通 report 仍可传递完整文本。
#[test]
fn direct_protocol_and_cli_apply_the_same_limit() {
    let reason = "字".repeat(51);
    let op = BrainOp::Decide {
        approval_id: "a".into(),
        decision: BrainDecision::Allow,
        reason: reason.clone(),
    };
    assert!(
        op.validate_decision_copy().is_err(),
        "直接协议也不能绕过摘要限制"
    );
    let parsed = norma::cli::parse_invocation_from([
        "norma", "brain", "decide", "a", "allow", "--reason", &reason,
    ]);
    assert!(parsed.is_err(), "CLI 必须在发送前拒绝超长理由");
    let report = BrainOp::Report {
        text: reason,
        priority: norma::protocol::ReportPriority::Normal,
        sources: vec![],
    };
    assert!(
        report.validate_decision_copy().is_ok(),
        "普通报告不受决策摘要预算影响"
    );
}
