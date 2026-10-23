mod run_overview_support;

use norma::mobile::{
    project_run_overview, status_capsule, RunOverviewAttentionKind, WorkspaceActivityFacts,
    WorkspaceActivityView,
};
use run_overview_support::{agent, facts, facts_json, files, presence};
use serde_json::json;

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

/// 场景：当前项目的 presence 缺项时，待处理行与状态分组对“已加载”的判断一致。
///
/// 数据构造：presence 列表里没有当前项目 area-current（只有未加载的设计草稿）；
///   当前项目里 Dee 出错、Bella 有一条可执行审批。
/// 执行过程：投影概览。
/// 预期结果：
///   - 正断言：Dee 的出错行和 Bella 的审批行 loaded 为真（当前项目一律视为已加载），
///     Bella 仍在运行中、Bruno 在待开始；设计草稿保持未加载。
///   - 负断言：不能出现“智能体在运行中却被当成未加载、定位按钮被禁用”的矛盾。
#[test]
fn current_project_is_loaded_for_attention_rows_even_without_presence() {
    let view = WorkspaceActivityView::from_facts(&facts());
    let only_other: Vec<_> = presence()
        .into_iter()
        .filter(|item| item.area_id != "area-current")
        .collect();
    let overview = project_run_overview("area-current", &view, &[], &only_other);
    let failed = overview
        .attention
        .iter()
        .find(|row| row.kind == RunOverviewAttentionKind::Failed)
        .expect("Dee 的出错行");
    assert!(failed.loaded, "当前项目的出错行必须可定位");
    let approval = overview
        .attention
        .iter()
        .find(|row| row.kind == RunOverviewAttentionKind::Approval && row.node_id == 1)
        .expect("Bella 的审批行");
    assert!(approval.loaded, "当前项目的审批行必须可处理");
    assert_eq!(
        overview
            .running
            .iter()
            .map(|a| a.node_id)
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(
        overview.idle.iter().map(|a| a.node_id).collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(overview.unloaded_projects.len(), 1, "设计草稿仍是未加载");
}

/// 场景：Ego 智能体在等你接管时，进入“需要你处理”，同时仍算“运行中”（任务还在进行，只是卡在等你）。
///
/// 数据构造：Bruno 改为 state=ego_awaiting_user，任务「检查抽屉响应式布局」；
///   另有 Eli（设计草稿，未加载）同样 ego_awaiting_user；Fay 为 ego_user_controlling（用户正在接管）。
/// 执行过程：投影概览与右上角胶囊。
/// 预期结果：
///   - 正断言：Bruno 有一行 Takeover，标题是任务名、已加载；Bruno 在运行中而不是待开始；
///     未加载项目的 Eli 也有 Takeover 行但 loaded 为假；待处理数与胶囊 pending_count 一致。
///   - 负断言：用户正在接管的 Fay 不进“需要你处理”（人已经在处理了）；Takeover 行排在审批之后、成果之前。
#[test]
fn awaiting_takeover_agents_need_attention_but_stay_running() {
    let mut value = facts_json();
    value["workspace"]["projects"][0]["agents"][1]["card"]["state"] = json!("ego_awaiting_user");
    let mut eli = agent(5, "ego_awaiting_user", "Eli", "整理品牌色板", false);
    eli["card"]["task_timing"] = json!(null);
    value["workspace"]["projects"][1]["agents"]
        .as_array_mut()
        .expect("设计草稿的智能体")
        .push(eli);
    let fay = agent(6, "ego_user_controlling", "Fay", "手动调整", false);
    value["workspace"]["projects"][0]["agents"]
        .as_array_mut()
        .expect("当前项目的智能体")
        .push(fay);
    let facts: WorkspaceActivityFacts = serde_json::from_value(value).expect("概览事实");
    let view = WorkspaceActivityView::from_facts(&facts);
    let overview = project_run_overview("area-current", &view, &files(), &presence());
    let takeovers: Vec<_> = overview
        .attention
        .iter()
        .filter(|row| row.kind == RunOverviewAttentionKind::Takeover)
        .collect();
    assert_eq!(
        takeovers.iter().map(|row| row.node_id).collect::<Vec<_>>(),
        vec![2, 5],
        "等待接管的两位都要处理，用户正在接管的 Fay 不算"
    );
    assert_eq!(takeovers[0].title, "检查抽屉响应式布局");
    assert!(takeovers[0].loaded, "当前项目里的接管可以定位");
    assert!(!takeovers[1].loaded, "未加载项目里的接管不能定位");
    assert!(
        overview.running.iter().any(|agent| agent.node_id == 2),
        "等待接管的智能体任务还在进行，算运行中"
    );
    assert!(
        overview.idle.iter().all(|agent| agent.node_id != 2),
        "不能同时出现在待开始"
    );
    let kinds: Vec<_> = overview.attention.iter().map(|row| row.kind).collect();
    let last_approval = kinds
        .iter()
        .rposition(|kind| *kind == RunOverviewAttentionKind::Approval)
        .expect("有审批");
    let first_takeover = kinds
        .iter()
        .position(|kind| *kind == RunOverviewAttentionKind::Takeover)
        .expect("有接管");
    let first_review = kinds
        .iter()
        .position(|kind| *kind == RunOverviewAttentionKind::Review)
        .expect("有成果");
    assert!(last_approval < first_takeover && first_takeover < first_review);
    let mut capsule = status_capsule(&facts, "area-current");
    capsule.apply_run_overview(&overview);
    assert_eq!(capsule.pending_count, overview.attention.len());
    assert_eq!(overview.summary().attention, overview.attention.len());
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
