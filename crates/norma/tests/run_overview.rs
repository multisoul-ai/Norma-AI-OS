use norma::mobile::{
    project_run_overview, status_capsule, RunOverviewAttentionKind, RunOverviewFile,
    RunOverviewPresence, RunOverviewProjectPresence, WorkspaceActivityFacts,
    WorkspaceActivityStatus, WorkspaceActivityView,
};
use serde_json::json;

/// 场景：运行概览按「需要你处理 → 运行中 → 待开始」归行，出错最先，其后审核先于成果、成果先于文件。
///
/// 数据构造：当前项目 area-current 名为 Norma-AI-OS。
///   Bella 节点 1，state=thinking，任务「调整原型布局与透明材质」，
///   待办 completed=4、total=6、current_step=原型交互验收，开始时间 1_700_000_000_000。
///   另有一条可执行审批，工具名 Bash，但行标题必须用任务名。
///   Bruno 节点 2，state=ready，没有待办。Dee 节点 4，state=failed，任务「失败的任务」。
///   其他项目 area-other 名为设计草稿，Cara 节点 3，state=waiting_approval，没有审批事实。
///   成果事实的 turn=turn-review，对应文件 docs/prototypes/norma-assistant/index.html。
///   另一个文件 src/left.css 属于 turn-file，因此单独成为一条成果。
///   当前项目 loaded=true；设计草稿 loaded=false，updated_at_unix_ms=280_000。
/// 执行过程：先投影视图，再按状态、加载情况分组。
/// 预期结果：
///   - 正断言：待处理顺序为 Dee 出错、Bella 审批、Cara 审批、turn-review 成果、turn-file 成果、两个文件；
///     运行中只有 Bella，待开始只有 Bruno；设计草稿是一条未加载摘要。
///   - 负断言：出错的 Dee 不在运行中也不在待开始；未加载项目的 Cara 不进状态分组；
///     Bruno 没有进度；分组结果不含「已运行」或「%」。
#[test]
fn overview_groups_attention_before_running_and_idle_agents() {
    let facts = facts();
    let view = WorkspaceActivityView::from_facts(&facts);
    let bella = &view.groups[0].agents[0];
    assert_eq!(
        bella
            .progress
            .as_ref()
            .map(|item| (item.completed, item.total)),
        Some((4, 6)),
        "待办完成数与总数要原样进入视图"
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
            RunOverviewAttentionKind::Failed,
            RunOverviewAttentionKind::Approval,
            RunOverviewAttentionKind::Approval,
            RunOverviewAttentionKind::Review,
            RunOverviewAttentionKind::Review,
            RunOverviewAttentionKind::File,
            RunOverviewAttentionKind::File,
        ],
        "待处理必须是出错、审核、成果、文件"
    );
    assert_eq!(
        (
            overview.attention[0].agent.as_str(),
            overview.attention[0].title.as_str(),
            overview.attention[0].project.as_str(),
            overview.attention[0].loaded
        ),
        ("Dee", "失败的任务", "Norma-AI-OS", true),
        "出错行写明智能体、任务与所属项目，当前项目可以定位"
    );
    assert_eq!(
        overview.attention[1].title, "调整原型布局与透明材质",
        "有任务名时审核事项不用工具名"
    );
    assert_eq!(
        overview.attention[1].agent, "Bella",
        "审核行要写提交的智能体"
    );
    assert_eq!(overview.attention[1].project, "Norma-AI-OS");
    assert_eq!(
        overview.attention[2].node_id, 3,
        "没有审批事实的待审智能体仍要占一行"
    );
    assert_eq!(overview.attention[2].title, "旧方案");
    assert!(
        !overview.attention[2].loaded,
        "未加载项目的审核行不能标成已加载"
    );
    assert_eq!(
        overview.attention[3].paths,
        vec!["docs/prototypes/norma-assistant/index.html".to_string()],
        "成果行只带该回合的文件"
    );
    assert_eq!(
        overview.attention[4].paths,
        vec!["src/left.css".to_string()],
        "没有成果事实的回合仍按文件归成一条成果"
    );
    assert_eq!(overview.attention[5].title, "index.html");
    assert_eq!(overview.attention[6].title, "left.css");
    assert!(
        overview
            .attention
            .iter()
            .all(|row| !row.title.contains('%')),
        "待处理标题不能出现百分比"
    );
    let running: Vec<_> = overview.running.iter().map(|agent| agent.node_id).collect();
    let idle: Vec<_> = overview.idle.iter().map(|agent| agent.node_id).collect();
    assert_eq!(running, vec![1], "运行中只有 Bella");
    assert_eq!(idle, vec![2], "待开始只有 Bruno");
    assert_eq!(overview.running[0].presence, RunOverviewPresence::Running);
    assert_eq!(
        overview.running[0].project, "Norma-AI-OS",
        "智能体行带所属项目名"
    );
    assert!(overview.running[0].current, "当前项目的智能体标为 current");
    assert_eq!(
        overview.running[0].approval_count, 1,
        "Bella 只有这一条待审核"
    );
    assert_eq!(
        overview.running[0]
            .progress
            .as_ref()
            .map(|item| item.completed),
        Some(4)
    );
    assert_eq!(overview.idle[0].presence, RunOverviewPresence::Idle);
    assert!(
        overview.idle[0].progress.is_none(),
        "没有待办的智能体不带进度"
    );
    assert_eq!(overview.idle[0].approval_count, 0, "Bruno 没有待审核");
    assert!(
        overview
            .running
            .iter()
            .chain(&overview.idle)
            .all(|agent| agent.node_id != 4 && agent.node_id != 3),
        "出错的 Dee 与未加载项目的 Cara 都不进状态分组"
    );
    assert_eq!(overview.unloaded_projects.len(), 1, "未加载项目收成摘要");
    let draft = &overview.unloaded_projects[0];
    assert_eq!(draft.name, "设计草稿");
    assert_eq!(draft.order, 2, "序号取项目在工作空间里的位置");
    assert_eq!(draft.agent_count, 1, "摘要要带智能体数");
    assert_eq!(
        draft.status,
        WorkspaceActivityStatus::Approval,
        "上次状态取项目里最紧急的状态"
    );
    assert!(!draft.loaded, "未加载项目保持未加载");
    assert_eq!(draft.updated_at_unix_ms, Some(280_000), "更新时间原样保留");
    let rendered = format!("{overview:?}");
    assert!(!rendered.contains("已运行"), "分组结果不能提前写成时长文案");
    assert!(!rendered.contains('%'), "分组结果不能出现百分比");
}

/// 场景：另一个项目已加载时，它的智能体跨项目进入状态分组，排序当前项目优先、运行更久的在前。
///
/// 数据构造：设计草稿 loaded=true。Bruno 改为 thinking，开始时间 1_600_000_000_000（比 Bella 早）。
///   Cara（设计草稿）state=waiting_approval，开始时间 1_700_000_000_000。
/// 执行过程：投影概览。
/// 预期结果：
///   - 正断言：运行中顺序为 Bruno、Bella、Cara；Cara 的项目名是设计草稿且 current=false；
///     设计草稿已加载，不再出现在 unloaded_projects；待开始为空。
///   - 负断言：Cara 虽在等审批，仍留在运行中（任务还在进行）。
#[test]
fn loaded_other_projects_join_running_with_current_project_first() {
    let mut value = facts_json();
    value["workspace"]["projects"][0]["agents"][1]["card"]["state"] = json!("thinking");
    value["workspace"]["projects"][0]["agents"][1]["card"]["task_timing"]["started_at_unix_ms"] =
        json!(1_600_000_000_000_u64);
    let facts: WorkspaceActivityFacts = serde_json::from_value(value).expect("概览事实");
    let view = WorkspaceActivityView::from_facts(&facts);
    let mut presence = presence();
    presence[1].loaded = true;
    let overview = project_run_overview("area-current", &view, &files(), &presence);
    let order: Vec<_> = overview
        .running
        .iter()
        .map(|agent| (agent.node_id, agent.current))
        .collect();
    assert_eq!(
        order,
        vec![(2, true), (1, true), (3, false)],
        "当前项目优先，组内运行更久的在前"
    );
    assert_eq!(overview.running[2].project, "设计草稿", "跨项目行带项目名");
    assert_eq!(
        overview.running[2].approval_count, 1,
        "等审批的智能体留在运行中并带徽标"
    );
    assert!(overview.idle.is_empty(), "没有空闲智能体时待开始为空");
    assert!(
        overview.unloaded_projects.is_empty(),
        "已加载项目的智能体逐个展开，不再留聚合行"
    );
}

/// 场景：右上角状态胶囊和抽屉用同一份概览投影，数字不会各算各的。
///
/// 数据构造：沿用第一个场景的事实（7 条待处理、Bella 运行中、Bruno 待开始、设计草稿未加载）。
/// 执行过程：先按事实生成胶囊，再把概览投影套到胶囊上。
/// 预期结果：
///   - 正断言：summary 为 待处理 7、运行中 1、待开始 1；胶囊的待处理数同步为 7；
///     读屏文字写明“共 2 个智能体，1 个运行中，1 个待开始”和“共 7 项待处理”。
///   - 负断言：未加载项目里的 Cara 不计入智能体数。
#[test]
fn status_capsule_counts_follow_the_run_overview() {
    let facts = facts();
    let view = WorkspaceActivityView::from_facts(&facts);
    let overview = project_run_overview("area-current", &view, &files(), &presence());
    let summary = overview.summary();
    assert_eq!(
        (summary.attention, summary.running, summary.idle),
        (7, 1, 1),
        "统计只数已加载项目的智能体"
    );
    let mut capsule = status_capsule(&facts, "area-current");
    capsule.apply_run_overview(&overview);
    assert_eq!(capsule.summary, summary);
    assert_eq!(capsule.pending_count, 7, "待处理数与概览一致");
    assert!(
        capsule
            .accessibility_label
            .contains("共 2 个智能体，1 个运行中，1 个待开始"),
        "读屏文字汇总智能体状态：{}",
        capsule.accessibility_label
    );
    assert!(capsule.accessibility_label.contains("共 7 项待处理"));
    assert!(
        !capsule.accessibility_label.contains("Cara"),
        "汇总不逐个点名"
    );
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
    serde_json::from_value(facts_json()).expect("概览事实")
}

fn facts_json() -> serde_json::Value {
    json!({
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
    })
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
