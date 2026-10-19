use norma::mobile::{
    WorkspaceActivityAgentEntry, WorkspaceActivityFacts, WorkspaceActivityPendingItem,
    WorkspaceActivityProjectGroup, WorkspaceActivityStatus, WorkspaceActivitySummary,
    WorkspaceActivityView,
};
use serde_json::json;

/// 场景：汇总行只留两名智能体时，全工作空间列表仍保留被截断的人，且运行/空闲与汇总判定一致。
///
/// 数据构造：同一项目 3 个智能体。
///   节点 2 的 state=thinking，对应 Running；节点 1 done、节点 3 ready，对应 Idle。
///   汇总按优先级稳定排序后只取前 2 名，隐藏数 = 3 - 2 = 1，被隐藏的是节点 3。
///   节点 1 的名称长 36 个 ASCII 字符，超过汇总摘录上限 30。
/// 执行过程：同一组事实分别交给 WorkspaceActivitySummary 与 WorkspaceActivityView。
/// 预期结果：
///   - 正断言：视图按输入顺序包含节点 1、2、3；2 为 Running，1 和 3 为 Idle；长名称原样保留。
///   - 负断言：汇总行不含节点 3；视图名称不是摘录后的短名。
#[test]
fn full_agent_list_keeps_agents_hidden_from_the_summary_row() {
    let name = "Agent-Name-That-Exceeds-Thirty-Chars";
    assert_eq!(name.len(), 36, "名称必须长于 30，才能证明全列表不做摘录");
    let facts = facts_with_agents(&[
        (1, "done", name),
        (2, "thinking", "Runner"),
        (3, "ready", "Idle"),
    ]);
    let summary = WorkspaceActivitySummary::from_facts("mac", facts.clone(), 100);
    let view = WorkspaceActivityView::from_facts(&facts);
    assert_eq!(
        summary.projects[0].agents.len(),
        2,
        "汇总行仍只保留两名智能体"
    );
    assert_eq!(
        summary.projects[0].other_agents, 1,
        "第三人计入 other_agents"
    );
    assert_eq!(summary.running_count, 1, "只有 thinking 计入运行数");
    assert!(
        summary.projects[0]
            .agents
            .iter()
            .all(|agent| agent.node_id != 3),
        "汇总行不得包含被截断的节点 3"
    );
    assert_eq!(view.agents.len(), 3, "全列表必须包含三名智能体");
    assert_eq!(
        view.agents
            .iter()
            .map(|agent| agent.node_id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3],
        "全列表保持输入顺序，不按优先级重排"
    );
    assert_eq!(
        view.agents[1].status,
        WorkspaceActivityStatus::Running,
        "thinking 必须判为 Running"
    );
    assert_eq!(
        view.agents[0].status,
        WorkspaceActivityStatus::Idle,
        "done 必须判为 Idle"
    );
    assert_eq!(
        view.agents[2].status,
        WorkspaceActivityStatus::Idle,
        "ready 必须判为 Idle"
    );
    assert_eq!(view.agents[1].raw_state, "thinking", "全列表保留原始状态");
    assert_eq!(view.agents[0].name, name, "全列表保留原始名称");
    let summarized = summary.projects[0]
        .agents
        .iter()
        .find(|agent| agent.node_id == 1)
        .expect("节点 1 仍在汇总的两名额内");
    assert_ne!(
        view.agents[0].name, summarized.name,
        "全列表不能改用汇总的摘录名"
    );
}

/// 场景：待处理事项只收录需要用户处理的审批和待验收，失败仍留在汇总计数里。
///
/// 数据构造：同一项目按输入顺序放三名智能体。
///   节点 4 有 1 条可执行审批，request_id = req-1，工具 Bash。
///   节点 7 状态 failed，没有审批也没有验收。
///   节点 2 有 1 份已完成、可验收、未冲突、未接受的交付，文件数 = 2。
/// 执行过程：用同一组事实投影汇总与视图。
/// 预期结果：
///   - 正断言：待处理依次为节点 4 的审批、节点 2 的验收；目标身份与原因使用给定字面量。
///   - 负断言：失败不进入待处理；汇总的失败计数仍为 1。
#[test]
fn pending_items_list_approvals_and_reviews_but_not_failures() {
    let fingerprint = "a".repeat(64);
    let facts: WorkspaceActivityFacts = serde_json::from_value(json!({
        "workspace": {
            "id": "space-1",
            "name": "我的工作空间",
            "revision": 1,
            "projects": [{
                "area_id": "area-1",
                "project_id": "project-1",
                "name": "当前项目",
                "root": "/local",
                "definitions": [],
                "agents": [
                    agent_json(4, "done", "Ada"),
                    agent_json(7, "failed", "Bea"),
                    agent_json(2, "done", "Cio")
                ]
            }]
        },
        "observed": true,
        "approvals": [{
            "area_id": "area-1",
            "approval": {
                "target": {
                    "node_id": 4,
                    "profile": "Codex",
                    "run_id": "run-1",
                    "session_id": "session-1",
                    "turn_id": "turn-1",
                    "request_id": "req-1"
                },
                "card_title": "Ada",
                "tool": "Bash",
                "details": {},
                "decisions": ["allow", "deny"]
            }
        }],
        "reviews": [{
            "area_id": "area-1",
            "node_id": 2,
            "profile": "Codex",
            "run_id": "old-run",
            "session_id": "old-session",
            "turn_id": "old-turn",
            "fingerprint": fingerprint,
            "completed": true,
            "available": true,
            "conflicted": false,
            "accepted": false,
            "file_count": 2
        }]
    }))
    .expect("待处理事实");
    let summary = WorkspaceActivitySummary::from_facts("mac", facts.clone(), 100);
    let view = WorkspaceActivityView::from_facts(&facts);
    assert_eq!(summary.failure_count, 1, "失败仍计入汇总");
    assert_eq!(summary.approval_count, 1, "审批仍计入汇总");
    assert_eq!(summary.review_count, 1, "待验收仍计入汇总");
    assert_eq!(
        view.pending,
        vec![
            WorkspaceActivityPendingItem {
                kind: "approval".into(),
                area_id: "area-1".into(),
                node_id: 4,
                project: "当前项目".into(),
                agent: "Ada".into(),
                reason: "Ada · 等待你确认 Bash".into(),
                request_id: Some("req-1".into()),
                fingerprint: None,
                run_id: "run-1".into(),
                session_id: "session-1".into(),
                turn_id: "turn-1".into(),
            },
            WorkspaceActivityPendingItem {
                kind: "review".into(),
                area_id: "area-1".into(),
                node_id: 2,
                project: "当前项目".into(),
                agent: "Cio".into(),
                reason: "2 个文件待验收".into(),
                request_id: None,
                fingerprint: Some(fingerprint),
                run_id: "old-run".into(),
                session_id: "old-session".into(),
                turn_id: "old-turn".into(),
            },
        ],
        "待处理必须按输入顺序给出审批和验收"
    );
    assert!(
        view.pending.iter().all(|item| item.node_id != 7),
        "失败智能体不能出现在待处理事项里"
    );
}

/// 场景：项目分组保持事实里的项目顺序和全部智能体，汇总仍按紧急程度只留三行。
///
/// 数据构造：四个项目按输入顺序为空闲、执行、审批、失败。
///   执行项目有 3 名智能体，其中 1 名 thinking；汇总每项目只留 2 名，因此该行 other_agents = 1。
///   四个项目里空闲优先级最低，三行预算使隐藏项目数 = 4 - 3 = 1。
/// 执行过程：同一组事实投影汇总与视图。
/// 预期结果：
///   - 正断言：分组顺序为 area-idle、area-run、area-approval、area-failed；执行分组含 3 人且状态为 Running。
///   - 负断言：分组第一项不是审批；汇总展示行不含空闲项目。
#[test]
fn project_groups_keep_source_order_while_summary_stays_capped() {
    let facts: WorkspaceActivityFacts = serde_json::from_value(json!({
        "workspace": {
            "id": "space-1",
            "name": "我的工作空间",
            "revision": 1,
            "projects": [
                project_json("area-idle", "idle", &[(1, "done", "Idle")]),
                project_json("area-run", "run", &[
                    (2, "done", "Done"),
                    (3, "thinking", "Runner"),
                    (4, "ready", "Ready")
                ]),
                project_json("area-approval", "approval", &[(5, "done", "Ada")]),
                project_json("area-failed", "failed", &[(6, "failed", "Bea")])
            ]
        },
        "observed": true,
        "approvals": [{
            "area_id": "area-approval",
            "approval": {
                "target": {
                    "node_id": 5,
                    "profile": "Codex",
                    "run_id": "run-1",
                    "session_id": "session-1",
                    "turn_id": "turn-1",
                    "request_id": "req-9"
                },
                "card_title": "Ada",
                "tool": "Bash",
                "details": {},
                "decisions": ["allow", "deny"]
            }
        }],
        "reviews": []
    }))
    .expect("分组事实");
    let summary = WorkspaceActivitySummary::from_facts("mac", facts.clone(), 100);
    let view = WorkspaceActivityView::from_facts(&facts);
    assert_eq!(summary.hidden_projects, 1, "四个项目只能展示三行");
    assert_eq!(
        summary
            .projects
            .iter()
            .map(|project| project.area_id.as_str())
            .collect::<Vec<_>>(),
        vec!["area-approval", "area-failed", "area-run"],
        "汇总仍按审批、失败、运行排序"
    );
    assert!(
        summary
            .projects
            .iter()
            .all(|project| project.area_id != "area-idle"),
        "空闲项目不能占汇总展示行"
    );
    let run = summary
        .projects
        .iter()
        .find(|project| project.area_id == "area-run")
        .expect("运行项目仍在汇总三行内");
    assert_eq!(run.agents.len(), 2, "汇总行仍只保留两名智能体");
    assert_eq!(
        view.groups,
        vec![
            group(
                "area-idle",
                "idle",
                WorkspaceActivityStatus::Idle,
                &[(1, "Idle", "done", WorkspaceActivityStatus::Idle)]
            ),
            group(
                "area-run",
                "run",
                WorkspaceActivityStatus::Running,
                &[
                    (2, "Done", "done", WorkspaceActivityStatus::Idle),
                    (3, "Runner", "thinking", WorkspaceActivityStatus::Running),
                    (4, "Ready", "ready", WorkspaceActivityStatus::Idle),
                ]
            ),
            group(
                "area-approval",
                "approval",
                WorkspaceActivityStatus::Approval,
                &[(5, "Ada", "done", WorkspaceActivityStatus::Approval)]
            ),
            group(
                "area-failed",
                "failed",
                WorkspaceActivityStatus::Failed,
                &[(6, "Bea", "failed", WorkspaceActivityStatus::Failed)]
            ),
        ],
        "分组必须保持输入顺序并带上每名智能体"
    );
    assert_ne!(
        view.groups[0].area_id, "area-approval",
        "项目分组不能改成汇总的紧急程度顺序"
    );
}

/// 场景：同一组事实上，验收状态与汇总一致，开始时间原样保留给渲染时换算时长。
///
/// 数据构造：一名已完成智能体，task_timing.started_at_unix_ms = 1_700_000_000_000。
///   另有 1 份可验收交付，文件数 = 1，因此状态应为 Review 而不是 Running。
///   1_700_000_000_000 毫秒只作为开始时刻，本投影不把它除以 60_000 写成「已运行 N 分」。
/// 执行过程：投影汇总与视图。
/// 预期结果：
///   - 正断言：智能体与所在项目都是 Review；开始时间、任务标题和 run/session/turn 与输入一致。
///   - 负断言：有开始时间也不能把已完成验收判成 Running。
#[test]
fn review_status_matches_summary_and_start_time_stays_unformatted() {
    let started_at_unix_ms = 1_700_000_000_000_u64;
    let mut facts = facts_with_agents(&[(8, "done", "Ada")]);
    facts.workspace.projects[0].agents[0].card.task_timing = Some(norma::mobile::TaskTiming {
        started_at_unix_ms,
        completed_elapsed_ms: None,
    });
    facts.workspace.projects[0].agents[0].card.task_title = Some("实现功能".into());
    facts.reviews.push(norma::mobile::ActivityReviewFact {
        area_id: "area-0".into(),
        node_id: 8,
        profile: "Codex".into(),
        run_id: "run-1".into(),
        session_id: "session-1".into(),
        turn_id: "turn-1".into(),
        fingerprint: "b".repeat(64),
        completed: true,
        available: true,
        conflicted: false,
        accepted: false,
        file_count: 1,
    });
    let summary = WorkspaceActivitySummary::from_facts("mac", facts.clone(), 100);
    let view = WorkspaceActivityView::from_facts(&facts);
    assert_eq!(
        summary.projects[0].status,
        WorkspaceActivityStatus::Review,
        "汇总项目必须是 Review"
    );
    assert_eq!(
        view.agents[0].status,
        WorkspaceActivityStatus::Review,
        "同一事实下智能体状态必须也是 Review"
    );
    assert_ne!(
        view.agents[0].status,
        WorkspaceActivityStatus::Running,
        "开始时间不能把待验收运行判成 Running"
    );
    assert_eq!(
        view.agents[0].started_at_unix_ms,
        Some(started_at_unix_ms),
        "开始时间必须原样保留，供渲染时换算"
    );
    assert_eq!(
        view.agents[0].task_title.as_deref(),
        Some("实现功能"),
        "任务标题必须原样保留"
    );
    assert_eq!(
        view.agents[0].run_id.as_deref(),
        Some("run-1"),
        "运行身份必须保留"
    );
    assert_eq!(
        view.agents[0].session_id.as_deref(),
        Some("session-1"),
        "会话身份必须保留"
    );
    assert_eq!(
        view.agents[0].turn_id.as_deref(),
        Some("turn-1"),
        "回合身份必须保留"
    );
}

fn group(
    area_id: &str,
    project_id: &str,
    status: WorkspaceActivityStatus,
    agents: &[(u64, &str, &str, WorkspaceActivityStatus)],
) -> WorkspaceActivityProjectGroup {
    WorkspaceActivityProjectGroup {
        area_id: area_id.into(),
        project_id: project_id.into(),
        name: project_id.into(),
        status,
        agents: agents
            .iter()
            .map(
                |(node_id, name, raw_state, status)| WorkspaceActivityAgentEntry {
                    area_id: area_id.into(),
                    node_id: *node_id,
                    name: (*name).into(),
                    raw_state: (*raw_state).into(),
                    status: *status,
                    task_title: Some("实现功能".into()),
                    progress: None,
                    started_at_unix_ms: None,
                    run_id: Some("run-1".into()),
                    session_id: Some("session-1".into()),
                    turn_id: Some("turn-1".into()),
                },
            )
            .collect(),
    }
}

fn project_json(area_id: &str, name: &str, agents: &[(u64, &str, &str)]) -> serde_json::Value {
    json!({
        "area_id": area_id,
        "project_id": name,
        "name": name,
        "root": "/local",
        "definitions": [],
        "agents": agents.iter().map(|(id, state, agent)| agent_json(*id, state, agent)).collect::<Vec<_>>()
    })
}

fn agent_json(id: u64, state: &str, name: &str) -> serde_json::Value {
    json!({
        "definition_id": id,
        "name": name,
        "profile": "Codex",
        "card": {
            "node_id": id,
            "profile": "Codex",
            "title": name,
            "mobile_control": true,
            "state": state,
            "run_id": "run-1",
            "session_id": "session-1",
            "turn_id": "turn-1",
            "task_title": "实现功能"
        }
    })
}

fn facts_with_agents(agents: &[(u64, &str, &str)]) -> WorkspaceActivityFacts {
    let agents = agents
        .iter()
        .map(|(id, state, name)| {
            json!({
                "definition_id": id,
                "name": name,
                "profile": "Codex",
                "card": {
                    "node_id": id,
                    "profile": "Codex",
                    "title": name,
                    "mobile_control": true,
                    "state": state,
                    "run_id": "run-1",
                    "session_id": "session-1",
                    "turn_id": "turn-1",
                    "task_title": "实现功能"
                }
            })
        })
        .collect::<Vec<_>>();
    serde_json::from_value(json!({
        "workspace": {
            "id": "space-1",
            "name": "我的工作空间",
            "revision": 1,
            "projects": [{
                "area_id": "area-0",
                "project_id": "project-0",
                "name": "当前项目",
                "root": "/local",
                "definitions": [],
                "agents": agents
            }]
        },
        "observed": true,
        "approvals": [],
        "reviews": []
    }))
    .expect("测试事实")
}
