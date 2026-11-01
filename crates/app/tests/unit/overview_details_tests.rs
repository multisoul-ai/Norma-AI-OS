use super::*;
use norma::protocol::{AgentProfile, QuestionRef, QuestionRequestId};

/// 场景：成功返回的关闭回执不能冒充当前决定已送达。
/// 构造：submitted、request_closed、waiting、未知状态与缺失状态的 JSON。
/// 执行：按宿主回执契约判断是否显示已允许/已拒绝。
/// 预期：只有明确 submitted 算成功，取消或其他终端关闭均不可误报。
#[test]
fn approval_receipt_requires_confirmed_submission() {
    assert!(
        approval_receipt_confirmed(&serde_json::json!({"status":"submitted"})),
        "准确送达才显示成功"
    );
    for status in ["request_closed", "waiting", "cancelled", "unknown"] {
        assert!(
            !approval_receipt_confirmed(&serde_json::json!({"status":status})),
            "{status} 不能说明当前决定生效"
        );
    }
    assert!(
        !approval_receipt_confirmed(&serde_json::json!({})),
        "缺失回执状态不能成功"
    );
}

fn question(revision: u64) -> DetailKey {
    DetailKey::Question {
        area_id: "a".into(),
        target: Box::new(QuestionRef {
            workspace_id: "desktop:w:a".into(),
            project_id: "p".into(),
            node_id: 7,
            provider: AgentProfile::Claude,
            run_id: "r".into(),
            connection_id: "c".into(),
            session_id: "s".into(),
            turn_id: None,
            request_id: QuestionRequestId::String("q".into()),
            revision,
        }),
    }
}

/// 场景：提议、问答和审批共用一个展开状态，原文的二级展开不能泄漏到下一项。
/// 构造：同一区域的一条提议、一题问题和一个审批身份。
/// 执行：依次展开并打开原文，再切换事项与收起。
/// 预期：最多一项展开；每次换项原文收起，按钮操作不需要改变本状态。
#[test]
fn attention_details_expand_one_stable_target_at_a_time() {
    let mut state = AttentionDetails::default();
    let proposal = DetailKey::Proposal(("a".into(), 1));
    let approval = DetailKey::Approval {
        area_id: "a".into(),
        node_id: 7,
        run_id: "r".into(),
        request_id: "p".into(),
    };
    assert!(state.open.is_none(), "默认全部收起");
    state.toggle(proposal.clone());
    state.toggle_original();
    assert!(
        state.is_open(&proposal) && state.original_open,
        "当前原文可展开"
    );
    state.toggle(question(1));
    assert!(!state.is_open(&proposal), "旧项必须收起");
    assert!(!state.original_open, "新项不能继承旧项原文展开状态");
    state.toggle(approval.clone());
    assert!(!state.is_open(&question(1)), "审批和问答也必须互斥");
    state.toggle(approval);
    assert!(state.open.is_none(), "再次点击当前项应收起");
}

/// 场景：刷新保留活跃状态，问题版本改变或工作空间切换必须撤销旧展开身份。
/// 构造：问题 revision=1/2，以及 w1/w2 两个工作空间。
/// 执行：以真实身份集合执行协调，模拟普通刷新、版本更新与工作空间切换。
/// 预期：普通刷新不收起，失效身份与另一个工作空间不能复用展开状态。
#[test]
fn attention_details_reconcile_workspace_and_question_revision() {
    let mut state = AttentionDetails::default();
    state.reconcile(Some("w1"), &[question(1)]);
    state.toggle(question(1));
    state.reconcile(Some("w1"), &[question(1)]);
    assert!(state.is_open(&question(1)), "普通刷新应保留当前项");
    state.reconcile(Some("w1"), &[question(2)]);
    assert!(state.open.is_none(), "新版本不能继承旧问题的展开与原文状态");
    state.toggle(question(2));
    state.reconcile(Some("w2"), &[question(2)]);
    assert!(state.open.is_none(), "同名区域在另一工作空间也不能串状态");
}

/// 场景：旧帧审批完成后保留即时回执，刷新失效身份或切换工作空间时清理。
/// 构造：两个不同请求身份，w1/w2 工作空间，以及一条成功允许的缓存记录。
/// 执行：普通刷新、替换当前请求、切换工作空间。
/// 预期：普通刷新保留回执以阻止旧帧重复操作；新请求和另一空间不能继承决定。
#[test]
fn approval_feedback_is_scoped_to_the_live_request_and_workspace() {
    let mut state = AttentionDetails::default();
    let old = DetailKey::Approval {
        area_id: "a".into(),
        node_id: 1,
        run_id: "r".into(),
        request_id: "old".into(),
    };
    let next = DetailKey::Approval {
        area_id: "a".into(),
        node_id: 1,
        run_id: "r".into(),
        request_id: "next".into(),
    };
    state.reconcile(Some("w1"), std::slice::from_ref(&old));
    state.decided.push((old.clone(), true));
    state.reconcile(Some("w1"), std::slice::from_ref(&old));
    assert_eq!(
        state.decided,
        [(old, true)],
        "原请求仍在旧帧时必须保留成功状态"
    );
    state.reconcile(Some("w1"), std::slice::from_ref(&next));
    assert!(state.decided.is_empty(), "下一次相同工具请求不能继承授权");
    state.decided.push((next.clone(), false));
    state.pending.push(next.clone());
    state.reconcile(Some("w2"), &[next]);
    assert!(state.decided.is_empty(), "另一工作空间不能显示旧回执");
    assert!(state.pending.is_empty(), "另一工作空间不能继承提交状态");
}

/// 场景：标题不能省略风险限定，短步骤只能来自原文的真实分行，不猜测长段落语义。
/// 构造：带否定的长标题、短标题、三条真实项目符号及超长步骤。
/// 执行：生成紧凑标题和短步骤投影。
/// 预期：短标题原样保留；长标题使用中性入口，完整句子不被硬截；短步骤保留否定。
#[test]
fn compact_copy_keeps_negation_and_never_slices_execution_text() {
    let long = format!("{}；不得推送", "核验".repeat(30));
    assert_eq!(
        compact_title("修复会话归因", "历史建议"),
        "修复会话归因",
        "短标题不改写"
    );
    assert_eq!(
        compact_title(&long, "历史建议"),
        "历史建议",
        "不能截掉长句末尾的否定"
    );
    let task = format!(
        "目标：复现\n- 核对官方协议\n- 保留未授权限制，不能推送\n- {}\n验收：cargo test",
        "长步骤".repeat(40)
    );
    let points = task_points(&task);
    assert_eq!(
        points,
        ["核对官方协议", "保留未授权限制，不能推送"],
        "只投影完整的短步骤"
    );
    assert!(
        task.ends_with("验收：cargo test"),
        "格式化不能改动完整任务载荷"
    );
}
