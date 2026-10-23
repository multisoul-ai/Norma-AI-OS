use super::present_overview;
use norma::mobile::{RunOverview, RunOverviewAgent, RunOverviewPresence};

fn agent(task: Option<&str>, latest: Option<&str>) -> RunOverviewAgent {
    RunOverviewAgent {
        area_id: "area-current".into(),
        project: "Norma-AI-OS".into(),
        current: true,
        node_id: 7,
        name: "Bella".into(),
        raw_state: "thinking".into(),
        presence: RunOverviewPresence::Running,
        approval_count: 0,
        task: task.map(str::to_string),
        latest_prompt: latest.map(str::to_string),
        action: None,
        progress: None,
        started_at_unix_ms: None,
    }
}

fn first_row(agent: RunOverviewAgent) -> super::OverviewRow {
    let overview = RunOverview {
        attention: Vec::new(),
        running: vec![agent],
        idle: Vec::new(),
        unloaded_projects: Vec::new(),
    };
    present_overview(&overview, &[], 1_800_000_000_000).sections[1].rows[0].clone()
}

/// 场景：Agent 行显示“会话名称 + 最新一条输入”，卡片名退到项目标签里。
///
/// 数据构造：
///   会话首条提示词「重构\n登录模块」（含换行），最新提示词「  顺便补上\n单元测试  」（含首尾空白与换行）。
/// 执行过程：用该智能体生成概览行。
/// 预期结果：
///   - 正断言：主标题是折成单行的会话名称「重构 登录模块」；
///     第二行是折成单行的最新输入「顺便补上 单元测试」；
///     项目标签同时保留项目与卡片名「Norma-AI-OS · Bella」。
///   - 负断言：主标题与第二行都不含换行；主标题不再是卡片名「Bella」。
#[test]
fn agent_row_shows_session_title_and_latest_prompt() {
    let row = first_row(agent(
        Some("重构\n登录模块"),
        Some("  顺便补上\n单元测试  "),
    ));
    assert_eq!(row.primary, "重构 登录模块", "主标题应是单行会话名称");
    assert_eq!(
        row.task_line.as_deref(),
        Some("顺便补上 单元测试"),
        "第二行应是单行最新输入"
    );
    assert_eq!(
        row.project_tag.as_deref(),
        Some("Norma-AI-OS · Bella"),
        "项目标签应保留项目和卡片名"
    );
    assert!(!row.primary.contains('\n'), "主标题不得含换行");
    assert_ne!(row.primary, "Bella", "有会话名称时主标题不再是卡片名");
}

/// 场景：只有一轮提示词，或还没有任何提示词时，不重复也不留空。
///
/// 数据构造：A 的首条与最新提示词相同；B 完全没有提示词；C 只有最新提示词为空串。
/// 执行过程：分别生成概览行。
/// 预期结果：
///   - 正断言：A 第二行为空（避免与标题重复）；B 与 C 主标题回退到卡片名「Bella」，项目标签只有项目名。
///   - 负断言：B、C 的第二行为空，不渲染空白占位。
#[test]
fn agent_row_falls_back_without_duplicating_title() {
    let same = first_row(agent(Some("重构登录"), Some("重构登录")));
    assert_eq!(same.primary, "重构登录");
    assert!(same.task_line.is_none(), "最新输入与标题相同时不重复显示");

    let none = first_row(agent(None, None));
    assert_eq!(none.primary, "Bella", "没有提示词时回退到卡片名");
    assert_eq!(none.project_tag.as_deref(), Some("Norma-AI-OS"));
    assert!(none.task_line.is_none(), "没有最新输入就不画第二行文字");

    let blank = first_row(agent(Some(""), Some("   ")));
    assert_eq!(blank.primary, "Bella", "空白标题回退到卡片名");
    assert!(blank.task_line.is_none(), "空白最新输入不显示");
}
