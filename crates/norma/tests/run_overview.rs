use norma::mobile::{
    project_run_overview, RunOverviewAttentionKind, RunOverviewFile, RunOverviewPresence,
    RunOverviewProjectPresence, WorkspaceActivityFacts, WorkspaceActivityStatus,
    WorkspaceActivityView,
};
use serde_json::json;

/// 场景：运行概览按「需要你处理 → 当前项目智能体 → 其他项目」归行，审核先于成果，成果先于文件。
///
/// 数据构造：当前项目 area-current 名为 Norma-AI-OS。
///   Bella 节点 1，state=thinking，任务「调整原型布局与透明材质」，
///   待办 completed=4、total=6、current_step=原型交互验收，开始时间 1_700_000_000_000。
///   另有一条可执行审批，工具名 Bash，但行标题必须用任务名。
///   Bruno 节点 2，state=ready，没有待办。Dee 节点 4，state=failed。
///   其他项目 area-other 名为设计草稿，Cara 节点 3，state=waiting_approval，没有审批事实。
///   成果事实的 turn=turn-review，对应文件 docs/prototypes/norma-assistant/index.html。
///   另一个文件 src/left.css 属于 turn-file，因此单独成为一条成果。
///   当前项目 loaded=true；设计草稿 loaded=false，updated_at_unix_ms=280_000。
/// 执行过程：先投影视图，再按当前项目、文件和加载状态分组。
/// 预期结果：
///   - 正断言：待处理顺序为 Bella 审批、Cara 审批、turn-review 成果、turn-file 成果、两个文件；
///     当前项目只有 Bella、Bruno、Dee；设计草稿是一条未加载摘要。
///   - 负断言：失败的 Dee 不在待处理里；Bruno 没有进度；分组结果不含「已运行」或「%」。
#[test]
fn overview_groups_attention_before_current_agents_and_other_projects() {
    let facts = facts();
    let view = WorkspaceActivityView::from_facts(&facts);
    let bella = &view.groups[0].agents[0];
    assert_eq!(
        bella.progress.as_ref().map(|item| item.completed),
        Some(4),
        "待办已完成数要原样进入视图"
    );
    assert_eq!(
        bella.progress.as_ref().map(|item| item.total),
        Some(6),
        "待办总数要原样进入视图"
    );
    assert_eq!(
        bella.started_at_unix_ms,
        Some(1_700_000_000_000),
        "开始时间留给渲染时换算"
    );
    let overview = project_run_overview("area-current", &view, &files(), &presence());
    let kinds: Vec<_> = overview.attention.iter().map(|row| row.kind).collect();
    assert_eq!(
        kinds,
        vec![
            RunOverviewAttentionKind::Approval,
            RunOverviewAttentionKind::Approval,
            RunOverviewAttentionKind::Review,
            RunOverviewAttentionKind::Review,
            RunOverviewAttentionKind::File,
            RunOverviewAttentionKind::File,
        ],
        "待处理必须是审核、成果、文件"
    );
    assert_eq!(
        overview.attention[0].title, "调整原型布局与透明材质",
        "有任务名时审核事项不用工具名"
    );
    assert_ne!(
        overview.attention[0].title, "Bash",
        "工具名 Bash 不能盖过任务名"
    );
    assert_eq!(
        overview.attention[0].agent, "Bella",
        "审核行要写提交的智能体"
    );
    assert_eq!(
        overview.attention[0].project, "Norma-AI-OS",
        "审核行要写所属项目"
    );
    assert!(overview.attention[0].loaded, "当前项目的审核可以定位");
    assert_eq!(
        overview.attention[1].node_id, 3,
        "没有审批事实的待审智能体仍要占一行"
    );
    assert_eq!(
        overview.attention[1].title, "旧方案",
        "合成审核行使用任务名"
    );
    assert!(
        !overview.attention[1].loaded,
        "未加载项目的审核行不能标成已加载"
    );
    assert_eq!(
        overview.attention[2].paths,
        vec!["docs/prototypes/norma-assistant/index.html".to_string()],
        "成果行只带该回合的文件"
    );
    assert_eq!(
        overview.attention[3].paths,
        vec!["src/left.css".to_string()],
        "没有成果事实的回合仍按文件归成一条成果"
    );
    assert_eq!(
        overview.attention[4].title, "index.html",
        "文件行的事项是文件名"
    );
    assert_eq!(
        overview.attention[5].title, "left.css",
        "第二个文件单独成行"
    );
    assert!(
        overview.attention.iter().all(|row| row.node_id != 4),
        "失败智能体不能进入需要你处理"
    );
    assert!(
        overview
            .attention
            .iter()
            .all(|row| !row.title.contains('%')),
        "待处理标题不能出现百分比"
    );
    assert_eq!(
        overview.current_project.as_deref(),
        Some("Norma-AI-OS"),
        "当前项目使用事实里的项目名"
    );
    assert_eq!(
        overview
            .current_agents
            .iter()
            .map(|agent| agent.node_id)
            .collect::<Vec<_>>(),
        vec![1, 2, 4],
        "当前项目智能体保持输入顺序"
    );
    assert_eq!(
        overview.current_agents[0].presence,
        RunOverviewPresence::Running,
        "thinking 显示为运行中"
    );
    assert_eq!(
        overview.current_agents[0].approval_count, 1,
        "Bella 只有这一条待审核"
    );
    assert_eq!(
        overview.current_agents[0]
            .progress
            .as_ref()
            .map(|item| item.completed),
        Some(4)
    );
    assert_eq!(
        overview.current_agents[1].presence,
        RunOverviewPresence::Idle,
        "ready 显示为待开始"
    );
    assert!(
        overview.current_agents[1].progress.is_none(),
        "没有待办的智能体不带进度"
    );
    assert_eq!(
        overview.current_agents[1].approval_count, 0,
        "Bruno 没有待审核"
    );
    assert_eq!(
        overview.current_agents[2].presence,
        RunOverviewPresence::Failed,
        "failed 显示为出错"
    );
    assert!(
        overview
            .current_agents
            .iter()
            .all(|agent| agent.node_id != 3),
        "其他项目的智能体不能进当前项目"
    );
    assert_eq!(
        overview.other_projects.len(),
        1,
        "其他项目收成摘要，不展开智能体"
    );
    assert_eq!(overview.other_projects[0].name, "设计草稿");
    assert_eq!(
        overview.other_projects[0].agent_count, 1,
        "摘要要带智能体数"
    );
    assert_eq!(
        overview.other_projects[0].status,
        WorkspaceActivityStatus::Approval,
        "上次状态取项目里最紧急的状态"
    );
    assert!(!overview.other_projects[0].loaded, "未加载项目保持未加载");
    assert_eq!(
        overview.other_projects[0].updated_at_unix_ms,
        Some(280_000),
        "更新时间原样保留"
    );
    assert!(
        overview
            .other_projects
            .iter()
            .all(|project| project.area_id != "area-current"),
        "当前项目不能再出现在其他项目"
    );
    let rendered = format!("{overview:?}");
    assert!(!rendered.contains("已运行"), "分组结果不能提前写成时长文案");
    assert!(!rendered.contains('%'), "分组结果不能出现百分比");
}

/// 场景：待审核没有任务名时，事项改用审批工具名。
///
/// 数据构造：节点 8 的 task_title 为空，审批工具为 Bash，request_id=req-tool。
/// 执行过程：投影概览。
/// 预期结果：标题是 Bash；不是空字符串，也不是整句原因。
#[test]
fn approval_without_task_uses_the_tool_name() {
    let mut facts = facts();
    facts.workspace.projects[0].agents[0].card.task_title = None;
    let view = WorkspaceActivityView::from_facts(&facts);
    let overview = project_run_overview("area-current", &view, &[], &presence());
    let approval = overview
        .attention
        .iter()
        .find(|row| row.node_id == 1)
        .expect("Bella 的审批还在");
    assert_eq!(approval.title, "Bash", "没有任务名时事项用工具名");
    assert_ne!(
        approval.title, "Ada · 等待你确认 Bash",
        "不能把整句原因当成事项"
    );
    assert!(!approval.title.is_empty(), "事项不能是空的");
}

fn presence() -> Vec<RunOverviewProjectPresence> {
    vec![
        RunOverviewProjectPresence {
            area_id: "area-current".into(),
            loaded: true,
            updated_at_unix_ms: None,
        },
        RunOverviewProjectPresence {
            area_id: "area-other".into(),
            loaded: false,
            updated_at_unix_ms: Some(280_000),
        },
    ]
}

fn files() -> Vec<RunOverviewFile> {
    vec![
        RunOverviewFile {
            area_id: "area-current".into(),
            node_id: 1,
            agent_hint: "Codex".into(),
            project_hint: String::new(),
            path: "docs/prototypes/norma-assistant/index.html".into(),
            turn_id: "turn-review".into(),
            fingerprint: "fp-index".into(),
        },
        RunOverviewFile {
            area_id: "area-current".into(),
            node_id: 1,
            agent_hint: "Codex".into(),
            project_hint: String::new(),
            path: "src/left.css".into(),
            turn_id: "turn-file".into(),
            fingerprint: "fp-css".into(),
        },
    ]
}

fn facts() -> WorkspaceActivityFacts {
    serde_json::from_value(json!({
        "workspace": {
            "id": "space-1",
            "name": "Norma OS",
            "revision": 1,
            "projects": [
                {
                    "area_id": "area-current",
                    "project_id": "norma",
                    "name": "Norma-AI-OS",
                    "root": "/local",
                    "definitions": [],
                    "agents": [
                        agent(1, "thinking", "Bella", "调整原型布局与透明材质", true),
                        agent(2, "ready", "Bruno", "检查抽屉响应式布局", false),
                        agent(4, "failed", "Dee", "失败的任务", false)
                    ]
                },
                {
                    "area_id": "area-other",
                    "project_id": "draft",
                    "name": "设计草稿",
                    "root": "/other",
                    "definitions": [],
                    "agents": [agent(3, "waiting_approval", "Cara", "旧方案", false)]
                }
            ]
        },
        "observed": true,
        "approvals": [{
            "area_id": "area-current",
            "approval": {
                "target": {
                    "node_id": 1,
                    "profile": "Codex",
                    "run_id": "run-1",
                    "session_id": "session-1",
                    "turn_id": "turn-1",
                    "request_id": "req-1"
                },
                "card_title": "Bella",
                "tool": "Bash",
                "details": {},
                "decisions": ["allow", "deny"]
            }
        }],
        "reviews": [{
            "area_id": "area-current",
            "node_id": 1,
            "profile": "Codex",
            "run_id": "run-review",
            "session_id": "session-review",
            "turn_id": "turn-review",
            "fingerprint": "fp-review",
            "completed": true,
            "available": true,
            "conflicted": false,
            "accepted": false,
            "file_count": 1
        }]
    }))
    .expect("概览事实")
}

fn agent(id: u64, state: &str, name: &str, task: &str, progress: bool) -> serde_json::Value {
    let mut card = json!({
        "node_id": id,
        "profile": "Codex",
        "title": name,
        "mobile_control": true,
        "state": state,
        "run_id": "run-1",
        "session_id": "session-1",
        "turn_id": "turn-1",
        "task_title": task,
        "task_timing": {"started_at_unix_ms": 1_700_000_000_000_u64, "completed_elapsed_ms": null}
    });
    if progress {
        card["task_progress"] = json!({"completed": 4, "total": 6, "current_step": "原型交互验收"});
    }
    json!({"definition_id": id, "name": name, "profile": "Codex", "card": card})
}
