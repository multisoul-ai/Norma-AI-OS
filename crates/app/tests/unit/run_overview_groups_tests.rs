#[path = "../support/drawer_locale.rs"]
mod drawer_locale;
use drawer_locale::expected_copy;

use super::{
    overview_subtitle, present_overview, OverviewAction, OverviewScreen, OverviewStatusTone,
};
use norma::mobile::{
    RunOverview, RunOverviewAgent, RunOverviewAttention, RunOverviewAttentionKind,
    RunOverviewPresence, RunOverviewProject, WorkspaceActivityStatus,
};

const NOW: u64 = 1_800_000_000_000;

/// 场景：出错的智能体进入「需要你处理」，用红色语义色，已加载时直接定位，未加载时只能切换项目。
///
/// 数据构造：Dee 在 Norma-AI-OS（已加载）任务「失败的任务」出错；Eli 在设计草稿（未加载）出错。
/// 执行过程：把两条出错事项交给概览行投影。
/// 预期结果：
///   - 正断言：主文案以「出错 ·」开头；色调为 Failed；Dee 行定位到智能体且可用；Eli 行动作是切换项目且可用。
///   - 负断言：出错行不是审核色调，也不带「待审核」文案。
#[test]
fn failed_agents_are_attention_rows_with_failed_tone() {
    let screen = present_overview(
        &overview(
            vec![
                failed("Dee", 4, "area-current", "Norma-AI-OS", "失败的任务", true),
                failed("Eli", 7, "area-other", "设计草稿", "导出样张", false),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ),
        &[],
        NOW,
    );
    let rows = &screen.sections[0].rows;
    assert_eq!(
        rows[0].primary,
        expected_copy("出错 · 失败的任务", "Error · 失败的任务"),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(rows[0].secondary, "Dee · Norma-AI-OS");
    assert_eq!(rows[0].status_tone, OverviewStatusTone::Failed);
    assert!(rows[0].enabled, "已加载项目里的出错智能体可以定位");
    assert!(matches!(
        rows[0].action,
        OverviewAction::LocateAgent { node_id: 4, .. }
    ));
    assert_eq!(
        rows[1].action_label,
        expected_copy("切换", "Switch"),
        "未加载项目只能先切换过去"
    );
    assert!(rows[1].enabled, "切换项目的动作始终可用");
    assert!(matches!(
        rows[1].action,
        OverviewAction::SwitchProject { ref area_id } if area_id == "area-other"
    ));
    assert_ne!(rows[0].status_tone, OverviewStatusTone::Approval);
    assert!(rows.iter().all(|row| row.approval_badge.is_none()));
}

/// 场景：等你接管的智能体在“需要你处理”里是琥珀色一行，已加载时定位到卡片，未加载时只能切换项目。
///
/// 数据构造：Bruno 在 Norma-AI-OS（已加载）等待接管，任务「检查抽屉响应式布局」；Eli 在设计草稿（未加载）等待接管。
/// 执行过程：把两条接管事项交给概览行投影。
/// 预期结果：
///   - 正断言：主文案以「等待接管 ·」开头，副文案写智能体与项目；色调为琥珀（Approval 色）；
///     Bruno 行定位到智能体且可用；Eli 行动作是切换项目且可用。
///   - 负断言：不带「待审核」徽标，也不使用出错的红色。
#[test]
fn takeover_rows_are_amber_and_locate_or_switch() {
    let mut loaded = failed(
        "Bruno",
        2,
        "area-current",
        "Norma-AI-OS",
        "检查抽屉响应式布局",
        true,
    );
    loaded.kind = RunOverviewAttentionKind::Takeover;
    let mut unloaded = failed("Eli", 5, "area-other", "设计草稿", "整理品牌色板", false);
    unloaded.kind = RunOverviewAttentionKind::Takeover;
    let screen = present_overview(
        &overview(vec![loaded, unloaded], Vec::new(), Vec::new(), Vec::new()),
        &[],
        NOW,
    );
    let rows = &screen.sections[0].rows;
    assert_eq!(
        rows[0].primary,
        expected_copy(
            "等待接管 · 检查抽屉响应式布局",
            "Awaiting takeover · 检查抽屉响应式布局"
        ),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(rows[0].secondary, "Bruno · Norma-AI-OS");
    assert_eq!(
        rows[0].status_tone,
        OverviewStatusTone::Approval,
        "与审批同一档紧急程度"
    );
    assert_ne!(rows[0].status_tone, OverviewStatusTone::Failed, "不是出错");
    assert!(rows[0].enabled);
    assert!(matches!(
        rows[0].action,
        OverviewAction::LocateAgent { node_id: 2, .. }
    ));
    assert_eq!(
        rows[1].action_label,
        expected_copy("切换", "Switch"),
        "未加载项目只能先切换过去"
    );
    assert!(rows[1].enabled);
    assert!(matches!(
        rows[1].action,
        OverviewAction::SwitchProject { ref area_id } if area_id == "area-other"
    ));
    assert!(rows.iter().all(|row| row.approval_badge.is_none()));
}

/// 场景：其他已加载项目里的智能体和当前项目的一样进入状态分组，用项目标签标明归属，点击直达卡片。
///
/// 数据构造：Cora 在设计草稿（已加载）运行中；Bruno 在 Norma-AI-OS 待开始，
///   另有一个未加载项目「归档」带 2 个智能体。
/// 执行过程：投影后读取各行。
/// 预期结果：
///   - 正断言：Cora 的项目标签是「设计草稿」，动作定位 area-other 的节点 5；
///     待开始组计数为 1，行数为 2（Bruno + 归档项目行）；项目行的序号是 03。
///   - 负断言：项目行不带项目标签；计数不含项目行。
#[test]
fn cross_project_agents_carry_a_project_tag_and_unloaded_rows_do_not_count() {
    let screen = present_overview(
        &overview(
            Vec::new(),
            vec![agent("Cora", 5, "area-other", "设计草稿", false)],
            vec![agent("Bruno", 8, "area-current", "Norma-AI-OS", true)],
            vec![RunOverviewProject {
                area_id: "area-archive".into(),
                order: 3,
                name: "归档".into(),
                agent_count: 2,
                status: WorkspaceActivityStatus::Idle,
                loaded: false,
                updated_at_unix_ms: None,
            }],
        ),
        &[],
        NOW,
    );
    let cora = &screen.sections[1].rows[0];
    assert_eq!(
        cora.project_tag.as_deref(),
        Some("设计草稿 · Cora"),
        "有会话名称时标签同时写项目与卡片名"
    );
    assert_eq!(cora.primary, "整理色板", "主标题是会话名称");
    assert!(matches!(
        cora.action,
        OverviewAction::LocateAgent { ref area_id, node_id: 5 } if area_id == "area-other"
    ));
    assert_eq!(screen.sections[2].count, Some(1), "待开始只数智能体");
    assert_eq!(screen.sections[2].rows.len(), 2, "项目行排在智能体之后");
    let project = &screen.sections[2].rows[1];
    assert_eq!(project.lead_label.as_deref(), Some("03"));
    assert!(
        project.project_tag.is_none(),
        "项目行本身就是项目，不再带标签"
    );
    assert_eq!(screen.summary(), [0, 1, 1], "统计卡与分组计数同源");
}

/// 场景：没有任何数据时三组都在，计数为 0，统计卡也是 0。
///
/// 数据构造：未投影的空屏。
/// 执行过程：读取分组标题、计数与统计。
/// 预期结果：标题依次是需要你处理、运行中、待开始；计数都为 Some(0)；summary 为 [0, 0, 0]。
#[test]
fn empty_screen_keeps_all_three_groups() {
    let screen = OverviewScreen::empty();
    let titles: Vec<_> = screen
        .sections
        .iter()
        .map(|item| item.title.as_str())
        .collect();
    assert_eq!(
        titles,
        [
            expected_copy("需要你处理", "Needs attention"),
            expected_copy("运行中", "Running"),
            expected_copy("待开始", "Not started")
        ],
        "产品文案应符合当前界面语言"
    );
    assert!(screen.sections.iter().all(|item| item.count == Some(0)));
    assert_eq!(screen.summary(), [0, 0, 0]);
}

/// 场景：概览副标题在有投影时间时追加「· 刚刚更新」，没有时间或没有名字时只退回名字。
#[test]
fn overview_subtitle_appends_update_age() {
    let now = 10_000_000u64;
    assert_eq!(
        overview_subtitle("Norma", Some(now as i64 - 5_000), now),
        expected_copy("Norma · 刚刚更新", "Norma · Just updated"),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(
        overview_subtitle("Norma", Some(now as i64 - 180_000), now),
        expected_copy("Norma · 3 分钟前更新", "Norma · Updated 3 min ago"),
        "产品文案应符合当前界面语言"
    );
    assert_eq!(overview_subtitle("Norma", None, now), "Norma");
    assert_eq!(overview_subtitle("", Some(now as i64), now), "");
}

fn overview(
    attention: Vec<RunOverviewAttention>,
    running: Vec<RunOverviewAgent>,
    idle: Vec<RunOverviewAgent>,
    unloaded_projects: Vec<RunOverviewProject>,
) -> RunOverview {
    RunOverview {
        attention,
        running,
        idle,
        unloaded_projects,
    }
}

fn failed(
    agent: &str,
    node_id: u64,
    area_id: &str,
    project: &str,
    title: &str,
    loaded: bool,
) -> RunOverviewAttention {
    RunOverviewAttention {
        kind: RunOverviewAttentionKind::Failed,
        area_id: area_id.into(),
        node_id,
        project: project.into(),
        agent: agent.into(),
        title: title.into(),
        run_id: String::new(),
        turn_id: String::new(),
        request_id: None,
        fingerprint: None,
        paths: Vec::new(),
        loaded,
    }
}

fn agent(
    name: &str,
    node_id: u64,
    area_id: &str,
    project: &str,
    current: bool,
) -> RunOverviewAgent {
    RunOverviewAgent {
        area_id: area_id.into(),
        project: project.into(),
        current,
        node_id,
        name: name.into(),
        raw_state: "thinking".into(),
        presence: if current {
            RunOverviewPresence::Idle
        } else {
            RunOverviewPresence::Running
        },
        approval_count: 0,
        task: Some("整理色板".into()),
        latest_prompt: None,
        action: None,
        progress: None,
        started_at_unix_ms: None,
    }
}
