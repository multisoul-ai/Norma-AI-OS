mod run_overview_support;

use norma::mobile::{
    project_run_overview, RunOverviewAttentionKind, RunOverviewPresence, WorkspaceActivityFacts,
    WorkspaceActivityStatus, WorkspaceActivityView,
};
use run_overview_support::{facts, facts_json, files, presence};
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

/// 场景：没有开始时间的智能体排在有开始时间的后面；当前项目不在视图里时不崩溃，也没有“当前”智能体。
///
/// 数据构造：Bruno 与 Bella 都 thinking，Bruno 没有开始时间，Bella 有；另一次把当前项目设成不存在的 area-missing。
/// 执行过程：投影概览。
/// 预期结果：
///   - 正断言：运行中顺序为 Bella、Bruno；area-missing 作为当前项目时所有已加载智能体的 current 为假。
///   - 负断言：不存在的当前项目不会让任何智能体被标成 current，也不会丢掉已加载项目的智能体。
#[test]
fn agents_without_start_time_sort_last_and_missing_current_project_is_safe() {
    let mut value = facts_json();
    value["workspace"]["projects"][0]["agents"][1]["card"]["state"] = json!("thinking");
    value["workspace"]["projects"][0]["agents"][1]["card"]["task_timing"] = json!(null);
    let facts: WorkspaceActivityFacts = serde_json::from_value(value).expect("概览事实");
    let view = WorkspaceActivityView::from_facts(&facts);
    let overview = project_run_overview("area-current", &view, &[], &presence());
    assert_eq!(
        overview
            .running
            .iter()
            .map(|a| a.node_id)
            .collect::<Vec<_>>(),
        vec![1, 2],
        "有开始时间的在前，没有的排最后"
    );
    let mut loaded = presence();
    loaded[1].loaded = true;
    let missing = project_run_overview("area-missing", &view, &[], &loaded);
    assert!(
        missing.running.iter().all(|agent| !agent.current),
        "不存在的当前项目下没有 current 智能体"
    );
    assert!(
        !missing.running.is_empty(),
        "已加载项目的智能体不能因此丢失"
    );
}
