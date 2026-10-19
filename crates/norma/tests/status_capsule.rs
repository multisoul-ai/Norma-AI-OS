use norma::mobile::{
    status_capsule, status_capsule_title, StatusCapsuleKind, StatusCapsuleRank,
    WorkspaceActivityFacts,
};
use serde_json::{json, Value};

/// 场景：右上角胶囊按「等你处理 > 出错 > 运行中 > 待开始」取前 3 个，空闲只进 +N。
///
/// 数据构造：当前项目 area-a 有 Bella（运行中，已完成 4/6，1 条待验收）、
///   Bruno（preparing，无待办）和 Ian（ready，空闲）。
///   其他项目 area-b「设计草稿」有 Cora（failed）和 Ada（thinking，无待办）。
///   Bella 另有 2 条验收和 1 条审批，待处理数 = 2 + 1 = 3。
///   已完成 4、总数 6，来自该智能体待办清单，不是估算。
/// 执行过程：用当前区域 area-a 调用 status_capsule。
/// 预期结果：可见顺序为 Bella、设计草稿 · Cora、设计草稿 · Ada；
///   Bruno 与 Ian 计入 +2；Bella 带琥珀标记且标题为「运行中 4/6」；待处理数为 3。
#[test]
fn capsule_ranks_current_project_and_keeps_three_plus_overflow() {
    let mut facts = facts(json!([
        project(
            "area-a",
            "Norma",
            json!([
                agent(1, "Bella", "thinking", Some((4, 6))),
                agent(2, "Bruno", "preparing", None),
                agent(3, "Ian", "ready", None),
            ])
        ),
        project(
            "area-b",
            "设计草稿",
            json!([
                agent(4, "Cora", "failed", None),
                agent(5, "Ada", "thinking", None),
            ])
        ),
    ]));
    facts.reviews.push(review("area-a", 1));
    facts.reviews.push(review("area-a", 1));
    facts.approvals.push(approval("area-a", 1));

    let capsule = status_capsule(&facts, "area-a");

    assert_eq!(capsule.chips.len(), 3, "胶囊最多显示 3 个智能体");
    assert_eq!(
        capsule.chips[0].display_name, "Bella",
        "等你处理的当前项目排第一"
    );
    assert_eq!(
        capsule.chips[0].rank,
        StatusCapsuleRank::Waiting,
        "有待验收的运行中智能体排序为等你处理"
    );
    assert!(capsule.chips[0].waiting_mark, "等你处理的环上要有琥珀标记");
    assert_eq!(capsule.chips[0].count_label, "4/6", "胶囊显示已完成数 4/6");
    assert_eq!(
        capsule.chips[0].title_label, "运行中 4/6",
        "卡片标题与胶囊已完成数一致"
    );
    assert_eq!(
        capsule.chips[0].completed,
        Some(4),
        "已完成数取待办清单的 4，而不是估算"
    );
    assert_eq!(capsule.chips[0].total, Some(6), "待办总数为 6");
    assert_eq!(
        capsule.chips[1].display_name, "设计草稿 · Cora",
        "非当前项目出错者排在运行中之前，并带项目名"
    );
    assert_eq!(
        capsule.chips[1].rank,
        StatusCapsuleRank::Failed,
        "失败排在运行中之前"
    );
    assert_eq!(
        capsule.chips[2].display_name, "设计草稿 · Ada",
        "第三个名额给其他项目的运行中智能体"
    );
    assert!(
        capsule
            .chips
            .iter()
            .all(|chip| chip.display_name != "Bruno" && chip.display_name != "Ian"),
        "待开始的第 4 名和空闲智能体不能占可见名额"
    );
    assert!(
        !capsule.chips[0].display_name.contains('·'),
        "当前项目不重复项目名"
    );
    assert_eq!(capsule.overflow, 2, "Bruno 与空闲 Ian 合成 +2");
    assert_eq!(
        capsule.pending_count, 3,
        "全工作空间待处理数是 1 条审批 + 2 条验收"
    );
    assert!(
        capsule
            .accessibility_label
            .contains("Bella 运行中，已完成 4/6"),
        "读屏要逐个说明状态和已完成数"
    );
    assert!(
        capsule.accessibility_label.contains("Bruno 待开始"),
        "未显示在前 3 的待开始仍要被读屏说明"
    );
    assert!(
        capsule.accessibility_label.contains("共 3 项待处理"),
        "读屏要说明全工作空间待处理数"
    );
    assert!(
        !capsule.accessibility_label.contains("Ian"),
        "空闲智能体不进入读屏名单"
    );
    let bella = capsule
        .titles
        .iter()
        .find(|title| title.node_id == 1)
        .expect("卡片标题表要包含 Bella");
    assert_eq!(
        bella.title_label, capsule.chips[0].title_label,
        "卡片标题字符串必须与胶囊芯片相同"
    );
}

/// 场景：同级智能体里当前项目优先，空闲只计入 +N。
///
/// 数据构造：当前项目 Yan 与其他项目 Zoe 都是 thinking，另有 4 个 ready。
///   没有待办进度，已完成数不适用。
/// 执行过程：以 area-now 为当前项目派生胶囊。
/// 预期结果：Yan 在 Zoe 之前；4 个空闲全部进入 +4，名字不出现。
#[test]
fn same_rank_prefers_current_project_and_idle_is_only_overflow() {
    let mut agents = vec![
        agent(1, "Yan", "thinking", None),
        agent(2, "Zoe", "thinking", None),
    ];
    for index in 3..=6 {
        agents.push(agent(index, &format!("Idle{index}"), "ready", None));
    }
    let facts = facts(json!([
        project("area-now", "当前", json!([agents[0].clone()])),
        project("area-other", "其他", json!(agents[1..].to_vec())),
    ]));
    let capsule = status_capsule(&facts, "area-now");
    assert_eq!(capsule.chips.len(), 2, "两个运行中的智能体都可见");
    assert_eq!(capsule.chips[0].display_name, "Yan", "同级时当前项目排前面");
    assert_eq!(
        capsule.chips[1].display_name, "其他 · Zoe",
        "同级的其他项目排在当前项目之后"
    );
    assert_ne!(
        capsule.chips[0].rank,
        StatusCapsuleRank::Waiting,
        "纯运行中不是等你处理"
    );
    assert_eq!(capsule.overflow, 4, "4 个空闲只计入 +N");
    assert!(
        capsule
            .chips
            .iter()
            .all(|chip| !chip.display_name.starts_with("Idle")),
        "空闲名字不能出现在胶囊里"
    );
    assert_eq!(capsule.pending_count, 0, "没有审批或验收时待处理数是 0");
}

/// 场景：没有完整待办清单时不显示分数，卡片标题用状态文字。
///
/// 数据构造：preparing 且没有进度；另一条进度 completed=5、total=3，5>3 视为缺口。
/// 执行过程：派生胶囊并单独调用标题函数。
/// 预期结果：显示「待开始」，不出现 0/1 或 5/3；标题函数对运行中 4/6 给出「运行中 4/6」。
#[test]
fn missing_or_broken_progress_uses_status_text_not_a_fraction() {
    let facts = facts(json!([project(
        "area-a",
        "Norma",
        json!([
            agent(1, "Bruno", "preparing", None),
            agent(2, "Gap", "thinking", Some((5, 3))),
        ])
    )]));
    let capsule = status_capsule(&facts, "area-a");
    let bruno = capsule
        .chips
        .iter()
        .find(|chip| chip.display_name == "Bruno")
        .expect("待开始应占一个名额");
    assert_eq!(bruno.count_label, "待开始", "没有待办清单时显示状态文字");
    assert_eq!(bruno.title_label, "待开始", "卡片标题同样是待开始");
    assert_eq!(bruno.completed, None, "不能伪造已完成数");
    assert_ne!(bruno.count_label, "0/1", "不能把未开始估成 0/1");
    let gap = capsule
        .chips
        .iter()
        .find(|chip| chip.display_name == "Gap")
        .expect("进度缺口的智能体仍应显示");
    assert_eq!(gap.count_label, "运行中", "已完成数大于总数时隐藏分数");
    assert_eq!(gap.completed, None, "缺口进度不能进入已完成数");
    assert_ne!(gap.count_label, "5/3", "不能展示不可能的 5/3");
    assert_eq!(
        status_capsule_title(StatusCapsuleKind::Running, Some(4), Some(6)),
        "运行中 4/6",
        "卡片标题函数与胶囊使用同一句已完成数"
    );
    assert_eq!(
        status_capsule_title(StatusCapsuleKind::PendingStart, None, None),
        "待开始",
        "没有进度时标题函数只留状态"
    );
}

fn facts(projects: Value) -> WorkspaceActivityFacts {
    serde_json::from_value(json!({
        "workspace": {"id": "space", "name": "工作空间", "revision": 1, "projects": projects},
        "observed": true,
        "approvals": [],
        "reviews": [],
    }))
    .expect("测试事实应能反序列化")
}

fn project(area: &str, name: &str, agents: Value) -> Value {
    json!({
        "area_id": area,
        "project_id": area,
        "name": name,
        "root": "/local",
        "definitions": [],
        "agents": agents,
    })
}

fn agent(node: u64, name: &str, state: &str, progress: Option<(u32, u32)>) -> Value {
    json!({
        "definition_id": node,
        "name": name,
        "profile": "Codex",
        "card": {
            "node_id": node,
            "profile": "Codex",
            "title": name,
            "mobile_control": true,
            "state": state,
            "run_id": "run-1",
            "session_id": "session-1",
            "turn_id": "turn-1",
            "task_progress": progress.map(|(completed, total)| json!({
                "completed": completed,
                "total": total,
                "current_step": "实现",
            })),
        }
    })
}

fn review(area: &str, node: u64) -> norma::mobile::ActivityReviewFact {
    norma::mobile::ActivityReviewFact {
        area_id: area.into(),
        node_id: node,
        profile: "Codex".into(),
        run_id: "run-1".into(),
        session_id: "session-1".into(),
        turn_id: "turn-1".into(),
        fingerprint: "a".repeat(64),
        completed: true,
        available: true,
        conflicted: false,
        accepted: false,
        file_count: 1,
    }
}

fn approval(area: &str, node: u64) -> norma::mobile::ActivityApprovalFact {
    serde_json::from_value(json!({
        "area_id": area,
        "approval": {
            "target": {
                "node_id": node,
                "profile": "Codex",
                "run_id": "run-1",
                "session_id": "session-1",
                "turn_id": "turn-1",
                "request_id": "req-1"
            },
            "card_title": "Agent",
            "tool": "Bash",
            "details": {},
            "decisions": ["allow", "deny"]
        }
    }))
    .expect("审批事实应能反序列化")
}
