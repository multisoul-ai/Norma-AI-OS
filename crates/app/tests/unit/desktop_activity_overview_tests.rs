use super::{overview_areas_from_activity, overview_task_text, ActivityPresentation};
use norma::mobile::{
    WorkspaceActivityAgentEntry, WorkspaceActivityProjectGroup, WorkspaceActivityStatus,
    WorkspaceActivityView,
};

/// 场景：概览行从共享投影读取，定位审批、停止 Ego 和过期项目的规则与原来一致。
///
/// 数据构造：调用方要求的区域顺序是 B、A、C。
///   A 有思考中的 Ada、待审的 Bea（run-1/turn-1）、可停止的 Ego（绑定 9）和不可停止的 Ego。
///   B 的 Bea 也是待审，但 presentation.live = false，updated_at_unix_ms = 2_000。
///   C 没有分组。Ada 的开始时间是 1_700_000_000_000，行文案不使用它。
/// 执行过程：用视图和 presentation 生成概览行，再单独换算过期任务文案。
/// 预期结果：
///   - 正断言：顺序为 B、A、C；实时待审可定位；Ego 只有绑定 9 时可停止；过期时间留到渲染时再加。
///   - 负断言：过期待审不能定位；思考中不需要审批；行任务不含「已运行」。
#[test]
fn overview_rows_follow_existing_locate_approval_and_ego_rules() {
    let mut view = WorkspaceActivityView {
        agents: vec![],
        pending: vec![],
        groups: vec![
            group(
                "area-a",
                vec![
                    entry("area-a", 1, "Ada", "thinking", Some("写测试"), None, None),
                    entry(
                        "area-a",
                        2,
                        "Bea",
                        "waiting_approval",
                        Some("审核登录方案"),
                        Some("run-1"),
                        Some("turn-1"),
                    ),
                    entry(
                        "area-a",
                        3,
                        "Ego Lite",
                        "ego_working",
                        Some("网页任务"),
                        None,
                        None,
                    ),
                    entry("area-a", 4, "Ego Lite", "ego_ready", None, None, None),
                ],
            ),
            group(
                "area-b",
                vec![entry(
                    "area-b",
                    5,
                    "Bea",
                    "waiting_approval",
                    Some("审核登录方案"),
                    Some("run-1"),
                    Some("turn-1"),
                )],
            ),
        ],
    };
    view.groups[0].agents[0].started_at_unix_ms = Some(1_700_000_000_000);
    let presentation = vec![
        ActivityPresentation {
            area_id: "area-a".into(),
            node_id: 1,
            live: true,
            updated_at_unix_ms: None,
            ego_stop_agent: None,
        },
        ActivityPresentation {
            area_id: "area-a".into(),
            node_id: 2,
            live: true,
            updated_at_unix_ms: None,
            ego_stop_agent: None,
        },
        ActivityPresentation {
            area_id: "area-a".into(),
            node_id: 3,
            live: true,
            updated_at_unix_ms: None,
            ego_stop_agent: Some(9),
        },
        ActivityPresentation {
            area_id: "area-a".into(),
            node_id: 4,
            live: true,
            updated_at_unix_ms: None,
            ego_stop_agent: None,
        },
        ActivityPresentation {
            area_id: "area-b".into(),
            node_id: 5,
            live: false,
            updated_at_unix_ms: Some(2_000),
            ego_stop_agent: Some(9),
        },
    ];
    let areas = overview_areas_from_activity(
        &["area-b".into(), "area-a".into(), "area-c".into()],
        &view,
        &presentation,
    );
    assert_eq!(
        areas
            .iter()
            .map(|area| area.area_id.as_str())
            .collect::<Vec<_>>(),
        vec!["area-b", "area-a", "area-c"],
        "概览区域必须跟随当前工作空间的项目顺序"
    );
    assert!(areas[2].rows.is_empty(), "没有智能体的项目不能编造行");
    let stale = &areas[0].rows[0];
    assert!(stale.needs_approval, "过期快照仍要显示待审");
    assert!(!stale.can_locate_approval, "未加载项目不能定位审批");
    assert!(stale.stale, "未加载项目必须标成过期");
    assert_eq!(stale.ego_stop_agent, None, "过期项目不能保留可停止的 Ego");
    assert_eq!(
        stale.updated_at_unix_ms,
        Some(2_000),
        "上次更新时间留给渲染换算"
    );
    assert_eq!(stale.task, "审核登录方案", "缓存任务不能提前拼上次更新");
    let live = &areas[1].rows;
    assert_eq!(live[0].state, "思考中", "thinking 的行文案不变");
    assert_eq!(live[0].task, "写测试", "任务标题原样显示");
    assert!(
        !live[0].task.contains("已运行"),
        "开始时间不能在缓存里换成时长文案"
    );
    assert!(!live[0].needs_approval, "思考中不是待审");
    assert!(live[1].needs_approval, "waiting_approval 仍是待审");
    assert!(
        live[1].can_locate_approval,
        "已加载且有 run/turn 的待审可以定位"
    );
    assert_eq!(live[1].run_id.as_deref(), Some("run-1"), "定位审批保留 run");
    assert_eq!(
        live[1].turn_id.as_deref(),
        Some("turn-1"),
        "定位审批保留 turn"
    );
    assert!(live[2].is_ego, "ego_working 仍是 Ego 行");
    assert_eq!(live[2].state, "执行中", "Ego 执行文案不变");
    assert_eq!(
        live[2].ego_stop_agent,
        Some(9),
        "可停止的 Ego 保留绑定的智能体"
    );
    assert!(live[3].is_ego, "ego_ready 仍是 Ego 行");
    assert_eq!(
        live[3].ego_stop_agent, None,
        "没有可停止任务时不提供停止目标"
    );
    assert_eq!(live[3].task, "暂无任务标题", "没有任务标题时沿用原占位");
    assert_eq!(live[3].state, "待命", "ego_ready 文案不变");
    assert_eq!(
        overview_task_text("审核登录方案", None),
        "审核登录方案",
        "实时行不追加上次更新"
    );
    let stale_text = overview_task_text("审核登录方案", Some(2_000));
    assert!(
        stale_text.starts_with("审核登录方案 · 上次更新"),
        "过期行在渲染时追加上次更新"
    );
    assert_ne!(
        stale_text, "审核登录方案",
        "过期行不能停在没有时间的任务标题"
    );
}

fn group(area_id: &str, agents: Vec<WorkspaceActivityAgentEntry>) -> WorkspaceActivityProjectGroup {
    WorkspaceActivityProjectGroup {
        area_id: area_id.into(),
        project_id: area_id.into(),
        name: area_id.into(),
        status: WorkspaceActivityStatus::Idle,
        agents,
    }
}

fn entry(
    area_id: &str,
    node_id: u64,
    name: &str,
    raw_state: &str,
    task: Option<&str>,
    run_id: Option<&str>,
    turn_id: Option<&str>,
) -> WorkspaceActivityAgentEntry {
    WorkspaceActivityAgentEntry {
        area_id: area_id.into(),
        node_id,
        name: name.into(),
        raw_state: raw_state.into(),
        status: WorkspaceActivityStatus::Idle,
        task_title: task.map(str::to_string),
        progress: None,
        started_at_unix_ms: None,
        run_id: run_id.map(str::to_string),
        session_id: None,
        turn_id: turn_id.map(str::to_string),
    }
}
