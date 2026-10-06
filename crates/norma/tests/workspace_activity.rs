use norma::mobile::{DesktopChatWorkspace, WorkspaceActivityFacts, WorkspaceActivitySummary};
use serde_json::json;

fn workspace() -> DesktopChatWorkspace {
    let projects = ["running", "review", "failed", "approval"].into_iter().enumerate().map(|(i, state)| {
        json!({"area_id":format!("area-{i}"),"project_id":format!("project-{i}"),"name":state,"root":"/local",
            "definitions":[],"agents":[{"definition_id":i+1,"name":format!("Agent-{i}"),"profile":"Codex",
            "card":{"node_id":i+1,"profile":"Codex","title":"Agent","mobile_control":true,
            "state":if state=="failed"{"failed"}else if state=="running"{"thinking"}else{"done"},
            "run_id":"run-1","session_id":"session-1","turn_id":"turn-1","task_title":"实现功能",
            "task_progress":if state=="running" {json!({"completed":2,"total":5,"current_step":"实现"})}else{json!(null)}}}]})
    }).collect::<Vec<_>>();
    serde_json::from_value(
        json!({"id":"space-1","name":"我的工作空间","revision":1,"projects":projects}),
    )
    .unwrap()
}

/// 三项目筛选：真实审批、失败和待验收优先于普通执行，隐藏项目仍进入总数。
///
/// 数据构造（含关键数值的推导过程）：
///   4 项目 × 1 Agent = 4 Agent；执行 1 + 审批 1 + 待验收 1 + 失败 1。
///   展示预算 = 3 项目，因此普通执行项目应隐藏，仍计入运行数 1。
/// 执行过程：
///   1. 输入目录、area-3 的真实审批、area-1 的未验收交付。
///   2. 汇总全部计数，再按审批、失败、验收排序并截取前三项。
///
/// 预期结果：
///   - 正断言：审批、失败、待验收依次存在，计数各为 1。
///   - 负断言：普通执行项目不占据前三行，不丢失其运行计数。
#[test]
fn selects_actionable_projects_without_losing_hidden_counts() {
    let facts: WorkspaceActivityFacts = serde_json::from_value(json!({"workspace":workspace(),"observed":true,
        "approvals":[{"area_id":"area-3","approval":{"target":{"node_id":4,"profile":"Codex","run_id":"run-1","session_id":"session-1","turn_id":"turn-1","request_id":"req-1"},"card_title":"Agent","tool":"Bash","details":{},"decisions":["allow","deny"]}}],
        "reviews":[{"area_id":"area-1","node_id":2,"profile":"Codex","run_id":"run-1","session_id":"session-1","turn_id":"turn-1","fingerprint":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","completed":true,"available":true,"conflicted":false,"accepted":false,"file_count":2}]})).unwrap();
    let summary = WorkspaceActivitySummary::from_facts("mac-test", facts, 100);
    assert_eq!(summary.projects.len(), 3, "锁屏必须只保留三个项目");
    assert_eq!(
        summary.projects[0].area_id, "area-3",
        "真实工具审批必须排第一"
    );
    assert_eq!(
        summary.projects[1].area_id, "area-2",
        "失败必须优先于待验收"
    );
    assert_eq!(
        summary.projects[2].area_id, "area-1",
        "已冻结未验收成果必须出现在第三行"
    );
    assert!(
        !summary.projects.iter().any(|p| p.area_id == "area-0"),
        "普通执行项目不应挤掉待处理项目"
    );
    assert_eq!(summary.running_count, 1, "隐藏的执行 Agent 仍应计入运行数");
    assert_eq!(summary.approval_count, 1, "真实待审批应计数一次");
    assert_eq!(summary.review_count, 1, "可验收未验收交付应计数一次");
}

/// 合并与生命周期：普通进展 30 秒，保活 60 秒，结束立即；八小时后必须先观察空闲。
/// 数据构造：起点 100s；100+29/30 为合并边界；100+60 为保活；100+8×3600 为系统上限。
/// 执行过程：启动 → 改进度 → 关键变化 → 保活 → 八小时结束 → 忙碌抑制 → 空闲后新启动。
/// 预期结果：正断言边界应更新；负断言边界前与未空闲的旧周期不得启动。
#[test]
fn coalesces_progress_and_never_restarts_the_eight_hour_cycle() {
    use norma::mobile::{activity_event, ActivityEvent};
    let mut current = WorkspaceActivitySummary::from_facts(
        "mac",
        WorkspaceActivityFacts {
            workspace: workspace(),
            observed: true,
            approvals: vec![],
            reviews: vec![],
        },
        100,
    );
    let previous = current.clone();
    assert_eq!(
        activity_event(&current, None, None, None, 0, false),
        Some(ActivityEvent::Start),
        "工作应启动一个周期"
    );
    current.projects[1].reason = "普通进展改变".into();
    current.updated_at = 129;
    assert_eq!(
        activity_event(&current, Some(&previous), Some(100), None, 100, false),
        None,
        "29秒普通进展应合并"
    );
    current.updated_at = 130;
    assert_eq!(
        activity_event(&current, Some(&previous), Some(100), None, 100, false),
        Some(ActivityEvent::Update),
        "30秒进展必须提交"
    );
    current = previous.clone();
    current.updated_at = 160;
    assert_eq!(
        activity_event(&current, Some(&previous), Some(100), None, 100, false),
        Some(ActivityEvent::Update),
        "60秒刷新有效期"
    );
    current.updated_at = 100 + 8 * 3600;
    assert_eq!(
        activity_event(&current, Some(&previous), Some(100), None, 100, false),
        Some(ActivityEvent::End),
        "八小时必须结束"
    );
    assert_eq!(
        activity_event(
            &current,
            Some(&previous),
            Some(100),
            Some(current.updated_at),
            current.updated_at,
            false
        ),
        None,
        "连续工作不能自动循环启动"
    );
    assert_eq!(
        activity_event(
            &current,
            Some(&previous),
            Some(100),
            Some(current.updated_at),
            current.updated_at,
            true
        ),
        Some(ActivityEvent::Start),
        "观察空闲后新工作可开新周期"
    );
    current.observed = false;
    assert_eq!(
        activity_event(&current, Some(&previous), Some(100), None, 100, false),
        None,
        "失去事实来源不能假装结束或恢复"
    );
}

/// Agent 进度和审批身份：相同卡片 ID 在另一项目不能冒用；审批消项后只保留实际进度。
/// 数据构造：2 项目 × 1 Agent；每个卡片 node=1，但 run-a/run-b，进度分别2/5和1/3。
/// 执行过程：输入迟到审批(run-old)和真实审批(run-b)→汇总→删除真实审批后再汇总。
/// 预期结果：run-old 不计数；审批消项且待处理清空；各 Agent 进度不交换、同类保持原顺序。
#[test]
fn preserves_agent_progress_and_removes_only_current_approvals() {
    let mut space = workspace();
    space.projects.truncate(2);
    space.projects[0].agents[0].card.state = "thinking".into();
    space.projects[1].agents[0].card.state = "thinking".into();
    space.projects[1].agents[0].card.node_id = 1;
    space.projects[1].agents[0].card.run_id = Some("run-b".into());
    space.projects[1].agents[0].card.task_progress = Some(norma::mobile::TaskProgress {
        completed: 1,
        total: 3,
        current_step: Some("检查".into()),
    });
    let approvals=serde_json::from_value(json!([
        {"area_id":"area-0","approval":{"target":{"node_id":1,"profile":"Codex","run_id":"run-old","session_id":"session-1","turn_id":"turn-1","request_id":"old"},"card_title":"A","tool":"Bash","details":{},"decisions":["allow","deny"]}},
        {"area_id":"area-1","approval":{"target":{"node_id":1,"profile":"Codex","run_id":"run-b","session_id":"session-1","turn_id":"turn-1","request_id":"live"},"card_title":"B","tool":"Bash","details":{},"decisions":["allow","deny"]}}
    ])).unwrap();
    let summary = WorkspaceActivitySummary::from_facts(
        "mac",
        WorkspaceActivityFacts {
            workspace: space.clone(),
            observed: true,
            approvals,
            reviews: vec![],
        },
        100,
    );
    assert_eq!(summary.approval_count, 1, "只有run-b当前审批可计数");
    assert_eq!(
        summary.attention[0].target.area_id, "area-1",
        "相同节点ID不能把审批映射到另一个项目"
    );
    assert_eq!(
        summary.projects[0].agents[0]
            .progress
            .as_ref()
            .unwrap()
            .total,
        3,
        "run-b进度保持自己的3步"
    );
    assert_eq!(
        summary.projects[1].agents[0]
            .progress
            .as_ref()
            .unwrap()
            .total,
        5,
        "run-a仍是自己的5步"
    );
    assert_eq!(
        summary.projects[0].agents[0].action, "1/3 · 检查",
        "审批项目中的进度必须属于run-b Agent"
    );
    assert_eq!(
        summary.projects[1].agents[0].action, "2/5 · 实现",
        "另一Agent保留自己的真实步骤，不能生成项目百分比"
    );
    let resolved = WorkspaceActivitySummary::from_facts(
        "mac",
        WorkspaceActivityFacts {
            workspace: space,
            observed: true,
            approvals: vec![],
            reviews: vec![],
        },
        101,
    );
    assert_eq!(resolved.approval_count, 0, "审批完成后立即消项");
    assert!(resolved.attention.is_empty(), "旧审批不应遗留在收件箱");
    assert_eq!(
        resolved.projects[0].area_id, "area-0",
        "同类执行中保持原区域顺序"
    );
    assert_eq!(
        norma::mobile::activity_event(&resolved, Some(&summary), Some(100), None, 100, false),
        Some(norma::mobile::ActivityEvent::Update),
        "消项是关键变化，1秒内也应提交"
    );
}

/// 完成不等于待验收：不可用、冲突、已验收以及别的 Provider 的交付不计入。
/// 数据构造：1个 done 卡片；4份排除成果及1份可验收成果，64位指纹。
/// 执行过程：空事实完成→分别输入四类排除成果→可验收交付→已验收消项。
/// 预期结果：完成卡片无待验收且不保活；只有有效未验收成果计入并保留活动。
#[test]
fn counts_only_frozen_reviewable_unaccepted_delivery() {
    let mut space = workspace();
    space.projects = vec![space.projects.remove(1)];
    let fact = norma::mobile::ActivityReviewFact {
        area_id: "area-1".into(),
        node_id: 2,
        profile: "Codex".into(),
        run_id: "run-1".into(),
        session_id: "session-1".into(),
        turn_id: "turn-1".into(),
        fingerprint: "a".repeat(64),
        completed: true,
        available: true,
        conflicted: false,
        accepted: false,
        file_count: 2,
    };
    let build = |reviews| {
        WorkspaceActivitySummary::from_facts(
            "mac",
            WorkspaceActivityFacts {
                workspace: space.clone(),
                observed: true,
                approvals: vec![],
                reviews,
            },
            100,
        )
    };
    assert_eq!(build(vec![]).review_count, 0, "done本身没有验收成果");
    assert!(!build(vec![]).active, "没有运行和待处理项应结束");
    let mut unavailable = fact.clone();
    unavailable.available = false;
    let mut conflicted = fact.clone();
    conflicted.conflicted = true;
    let mut accepted = fact.clone();
    accepted.accepted = true;
    let mut other = fact.clone();
    other.profile = "Claude".into();
    assert_eq!(
        build(vec![unavailable, conflicted, accepted.clone(), other]).review_count,
        0,
        "不可用、冲突、验收完和错Provider全部排除"
    );
    let pending = build(vec![fact]);
    assert_eq!(pending.review_count, 1, "冻结且可验收成果计数");
    assert!(pending.active, "无人执行但有待验收时应保留");
    assert!(!build(vec![accepted]).active, "验收最后成果后应结束");
}

/// 审批去重包含节点身份：同一区域的两个Agent可拥有相同run和requestID。
/// 数据构造：1项目×2Agent，node=1/9；两条审批同run/session/turn/request但节点不同。
/// 执行过程：克隆Agent并绑定第二个节点→输入各自审批→汇总全部身份。
/// 预期结果：两条审批都保留，不能按run/request把不同Agent合并为一条。
#[test]
fn approvals_with_shared_provider_ids_keep_distinct_agents() {
    let mut space = workspace();
    space.projects.truncate(1);
    let mut second = space.projects[0].agents[0].clone();
    second.card.node_id = 9;
    second.name = "另一个Agent".into();
    space.projects[0].agents.push(second);
    let approvals=[1,9].map(|node|serde_json::from_value(json!({"area_id":"area-0","approval":{"target":{"node_id":node,"profile":"Codex","run_id":"run-1","session_id":"session-1","turn_id":"turn-1","request_id":"request"},"card_title":"Agent","tool":"Bash","details":{},"decisions":["allow","deny"]}})).unwrap()).to_vec();
    let summary = WorkspaceActivitySummary::from_facts(
        "mac",
        WorkspaceActivityFacts {
            workspace: space,
            observed: true,
            approvals,
            reviews: vec![],
        },
        100,
    );
    assert_eq!(summary.approval_count, 2, "两个不同节点必须各计一次审批");
    assert!(
        summary.attention.iter().any(|a| a.target.node_id == 1),
        "第一个Agent审批必须保留"
    );
    assert!(
        summary.attention.iter().any(|a| a.target.node_id == 9),
        "第二个Agent审批不得被误去重"
    );
    assert!(
        !summary.attention.iter().any(|a| a.target.node_id == 2),
        "不能合成不存在的Agent身份"
    );
}
