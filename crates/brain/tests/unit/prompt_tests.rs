use super::*;

/// 场景：完成事件与会话首条提示都约束决策文案，不能只在首轮要求简短。
/// 构造：主/审批 primer 及静默/需要告知两种完成事件。
/// 执行：检查 50 字符预算、明确行动及保留完整任务的要求。
/// 预期：四种入口均要求简洁，下一步组合预算不变，不能截断完整指令。
#[test]
fn decision_prompts_require_short_clear_copy() {
    let lines = ["- Henry 已完成：需要补测试".to_string()];
    for text in [
        primer(Role::Main, "/bin/norma", None),
        primer(Role::Approval, "/bin/norma", None),
        urgent_batch(&lines),
        quiet_batch(&lines),
    ] {
        assert!(
            text.contains("50 个字符"),
            "每个入口都需明确长度上限：{text}"
        );
        assert!(
            text.contains("简洁明确"),
            "每个入口都需明确内容质量：{text}"
        );
        assert!(
            text.contains("不能直接截断"),
            "不能通过删掉句尾的风险限定满足预算：{text}"
        );
    }
    let next = quiet_batch(&lines);
    assert!(next.contains("title 与 why"), "下一步应约束两个字段合计");
    assert!(next.contains("task 不受"), "完整执行载荷不受文案预算限制");
}

/// 场景：审批短理由仍需评估风险，不能把简洁要求误当作默认放行。
/// 构造：项目中 cargo test 请求，审批标识 a1。
/// 执行：构造审批提示并检查理由长度和升级要求。
/// 预期：明确要求 50 字符；拿不准继续 escalate，命令保持完整。
#[test]
fn approval_copy_budget_keeps_risk_and_escalation() {
    let request = ApprovalRequest {
        id: "a1".into(),
        agent: "Codex".into(),
        subject: crate::risk::ApprovalSubject {
            tool_name: "Bash".into(),
            command: Some("cargo test -p norma".into()),
            file_path: None,
            project_root: "/work/project".into(),
        },
    };
    let text = approval(&request, "/bin/norma");
    assert!(text.contains("50 个字符"), "审批理由应限制长度");
    assert!(text.contains("拿不准就 escalate"), "简短文案不改变风险兜底");
    assert!(
        text.contains("cargo test -p norma"),
        "原始请求不得被文案要求改写"
    );
}

/// 大脑只提议下一步，不直接派活：完成提示和首条说明都指向 propose。
///
/// 数据构造：一条完成记录，分别渲染“需要告知”“静默记录”两种提示；再取主会话首条说明。
/// 预期结果：
///   - 正断言：三段文字都含 propose；完成提示要求写清怎样算完成，并优先选空闲卡片
///   - 负断言：三段文字都不含 assign
#[test]
fn prompts_propose_instead_of_assign() {
    let lines = ["- Alan 已完成：写好了".to_string()];
    let texts = [
        urgent_batch(&lines),
        quiet_batch(&lines),
        primer(Role::Main, "/bin/norma", None),
    ];
    for text in &texts {
        assert!(text.contains("propose"), "{text}");
        assert!(!text.contains("assign"), "不应再让大脑直接派活：{text}");
    }
    for text in &texts[..2] {
        assert!(text.contains("怎样算完成"), "{text}");
        assert!(text.contains("空闲"), "{text}");
    }
}

/// 用户没采纳的提议附在后续提示里，避免反复提。
///
/// 数据构造：一段提示文字与一段“没采纳”清单。
/// 预期结果：
///   - 正断言：结果先是原提示，后面是清单，并说明不要再提同样的事
///   - 负断言：原提示不被改写
#[test]
fn declined_proposals_are_appended() {
    let text = with_declined("【事件】x", "- Alan：补测试");
    assert!(text.starts_with("【事件】x\n"), "{text}");
    assert!(text.contains("- Alan：补测试"), "{text}");
    assert!(text.contains("不要再提"), "{text}");
}

/// 首条说明总是和第一条事件拼在一起发出，不能再叫大脑“只回复收到”，否则第一条事件会被忽略。
///
/// 数据构造：主会话与审批会话的首条说明。
/// 预期结果：
///   - 正断言：仍说明只能用 norma 命令行动
///   - 负断言：不含“回复“收到”即可”
#[test]
fn primer_does_not_tell_the_brain_to_only_acknowledge() {
    for role in [Role::Main, Role::Approval] {
        let text = primer(role, "/bin/norma", None);
        assert!(text.contains("只能通过终端执行"), "{text}");
        assert!(!text.contains("回复“收到”即可"), "{text}");
    }
}

/// 完成提示每轮都要给出下一步的结论：要么提议，要么说明为什么没有；不能只回“收到”。
///
/// 数据构造：一条完成记录，渲染“需要告知”“静默记录”两种提示。
/// 预期结果：
///   - 正断言：都要求最后写“已提议：”或“没有下一步：”；都说明提议不会打扰用户；
///     都说明“按要求没做”不代表以后不要
///   - 负断言：都不再出现“只回复“收到””
#[test]
fn finished_prompts_require_a_next_step_verdict() {
    let lines = ["- Alan 已完成：写好了，还没有测试".to_string()];
    for text in [urgent_batch(&lines), quiet_batch(&lines)] {
        assert!(text.contains("已提议："), "{text}");
        assert!(text.contains("没有下一步："), "{text}");
        assert!(text.contains("不会打扰用户"), "{text}");
        assert!(text.contains("不代表以后不要"), "{text}");
        assert!(text.contains("刚完成任务的那个 Agent 本人"), "{text}");
        assert!(!text.contains("优先选有卡片且空闲"), "{text}");
        assert!(!text.contains("只回复“收到”"), "{text}");
    }
}

/// 路径含空格时，提示里的命令必须加引号，闸门才能解析出完整的可执行文件路径。
/// 数据构造：App 包路径 `/Applications/Norma OS.app/Contents/MacOS/norma`。
/// 预期结果：primer 中的命令被单引号包住，普通路径保持原样。
#[test]
fn bin_path_with_spaces_is_quoted_in_primer() {
    let text = primer(
        Role::Main,
        "/Applications/Norma OS.app/Contents/MacOS/norma",
        None,
    );
    assert!(text.contains("'/Applications/Norma OS.app/Contents/MacOS/norma' brain report"));
    assert!(primer(Role::Main, "/bin/norma", None).contains("`/bin/norma` 命令"));
}
