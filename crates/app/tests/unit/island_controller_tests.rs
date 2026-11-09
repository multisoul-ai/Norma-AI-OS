use super::*;
use gpui::{AppContext, TestAppContext};
use runtime::island::{IslandAlert, IslandAlertKind};

/// 场景：离开悬停区域已触发逻辑收起，但120ms视觉收回尚未结束时用户按Esc。
/// 数据：空概览控制器，指针进入后展开、离开；单调时钟推进500ms，越过420ms截止时间。
/// 执行：触发Tick使域状态收起，再发送不再改变域状态的Esc。
/// 预期：Esc仍向窗口发布新的即时呈现版本，令剩余形变立刻结束；域状态保持收起。
#[test]
fn island_escape_interrupts_presentation_after_logical_collapse() {
    let mut cx = TestAppContext::single();
    let model = cx.new(|_| IslandModel::default());
    let controller = cx.new(|cx| IslandController::new(model, cx));
    controller.update(&mut cx, |state, cx| {
        state.event(1, IslandEvent::PointerEntered, cx);
        state.event(1, IslandEvent::Expand, cx);
        state.event(1, IslandEvent::PointerLeft, cx);
        state.epoch -= Duration::from_millis(500);
        state.event(1, IslandEvent::Tick, cx);
        assert_eq!(
            state.displays.state.mode(),
            runtime::island::IslandMode::Compact,
            "已越过420ms收回截止"
        );
        let before = state.instant_revision;
        state.event(1, IslandEvent::Escape, cx);
        assert_ne!(
            state.instant_revision, before,
            "状态未变的Esc仍需结束正在播放的形变"
        );
        assert_eq!(
            state.displays.state.mode(),
            runtime::island::IslandMode::Compact,
            "Esc不能重新展开"
        );
    });
}

use crate::test_run_overview_rows as rows;

/// 场景：出错与需接管只出现在运行页，不能生成一个空的“需要你处理”徽标。
/// 数据：同一空间依次只含Failed、Takeover、Review或File，且没有运行行/审批/问题。
/// 执行：使用生产概览数据构造，每次计算岛的运行/待办计数。
/// 预期：出错和接管为零待办；成果和文件各为一件，确保真正待办不被一并清掉。
#[test]
fn island_pending_count_matches_rows_that_pending_tab_renders() {
    use norma::mobile::RunOverviewAttentionKind::{Failed, File, Review, Takeover};
    for (kind, pending) in [(Failed, 0), (Takeover, 0), (Review, 1), (File, 1)] {
        let mut overview = rows::sample(2_000_000);
        overview.running.clear();
        overview.idle.clear();
        overview.unloaded_projects.clear();
        overview.attention.truncate(1);
        overview.attention[0].kind = kind;
        let workspace = IslandWorkspace {
            database: "db.sqlite".into(),
            id: "w".into(),
            name: "Workspace".into(),
            projects: vec![super::super::IslandProject {
                source: 1,
                area_id: "a".into(),
                name: "Project".into(),
                root: "/project".into(),
                loaded: true,
                cached_overview: overview.clone(),
                overview,
                approvals: vec![],
                questions: vec![],
                proposals: vec![],
            }],
        };
        assert_eq!(
            counts(&[workspace]),
            (0, pending),
            "{kind:?}徽标必须与待办页实际行一致"
        );
    }
}

/// 场景：操作反馈收回后，另一个任务完成；旧成功文案不能污染新的Activity。
/// 数据：真实Controller保存回复已提交文案，ActionConfirmed启动2200ms计时。
/// 执行：推进2300ms再Tick，随后Completed；检查消息和当前来源。
/// 预期：任务完成身份生效且旧操作反馈被清掉。
#[test]
fn island_completion_clears_prior_action_feedback() {
    let mut cx = TestAppContext::single();
    let model = cx.new(|_| IslandModel::default());
    let controller = cx.new(|cx| IslandController::new(model, cx));
    controller.update(&mut cx, |state, cx| {
        state.message = Some(ui::i18n::LocalizedText::new("island.reply_submitted"));
        state.event(1, IslandEvent::ActionConfirmed("q".into()), cx);
        state.epoch -= Duration::from_millis(2300);
        state.event(1, IslandEvent::Tick, cx);
        assert_eq!(
            state.displays.state.mode(),
            runtime::island::IslandMode::Compact,
            "反馈先正常收回"
        );
        state.event(1, IslandEvent::Completed("task-2".into()), cx);
        assert_eq!(
            state.displays.state.activity_id(),
            Some("task-2"),
            "新任务拥有Activity"
        );
        assert!(state.message.is_none(), "不能继续画旧回复成功行");
    });
}

/// 场景：展开列表确认审批后，后台投影仍暂时保留旧请求。
/// 数据：一个审批与一个Review成果；生产Controller刷新/确认路径。
/// 执行：确认审批后保持原快照并重新刷新，再读取渲染/挂载共用快照。
/// 预期：旧审批不出现、徽标只计成果，原始快照保留；新快照移除后集合可回收。
#[test]
fn island_confirmed_approval_is_hidden_from_stale_expanded_projection() {
    use norma::mobile::{ApprovalSnapshot, ApprovalTarget, RunOverviewAttentionKind};
    let mut overview = rows::sample(2_000_000);
    overview.running.clear();
    overview.attention.truncate(1);
    overview.attention[0].kind = RunOverviewAttentionKind::Review;
    let approval = ApprovalSnapshot {
        target: ApprovalTarget {
            node_id: 7,
            profile: "Claude".into(),
            run_id: "r".into(),
            session_id: "s".into(),
            turn_id: "t".into(),
            request_id: "a1".into(),
        },
        card_title: "Agent".into(),
        tool: "Bash".into(),
        details: serde_json::json!({}),
        decisions: vec![norma::agent_hook::ApprovalDecision::Allow],
    };
    let id = alert_key(1, "w", "a", &approval.target);
    let workspace = IslandWorkspace {
        database: "db.sqlite".into(),
        id: "w".into(),
        name: "Workspace".into(),
        projects: vec![super::super::IslandProject {
            source: 1,
            area_id: "a".into(),
            name: "Project".into(),
            root: "/project".into(),
            loaded: true,
            cached_overview: overview.clone(),
            overview,
            approvals: vec![approval],
            questions: vec![],
            proposals: vec![],
        }],
    };
    let mut cx = TestAppContext::single();
    let model = cx.new(|_| {
        let mut model = IslandModel::default();
        model.workspaces = vec![workspace];
        model
    });
    let controller = cx.new(|cx| IslandController::new(model.clone(), cx));
    controller.update(&mut cx, |state, cx| {
        state.event(1, IslandEvent::Expand, cx);
        assert_eq!(
            counts(&state.visible_workspaces(cx)),
            (0, 2),
            "初始审批和成果均可见"
        );
        state.action_feedback(id, true, cx);
        state.refresh(cx);
        let visible = state.visible_workspaces(cx);
        assert!(
            visible[0].projects[0].approvals.is_empty(),
            "旧投影不能重新启用已确认审批"
        );
        assert_eq!(counts(&visible), (0, 1), "徽标只计剩余成果");
        assert_eq!(
            model.read(cx).workspaces[0].projects[0].approvals.len(),
            1,
            "测试必须真的保留旧投影"
        );
        assert_eq!(
            state.displays.state.mode(),
            runtime::island::IslandMode::Expanded,
            "列表不强制收回"
        );
    });
}

/// 场景：确认反馈自动结束或被下一条提醒抢占，旧成功文案不能留在新题上。
/// 数据：生产Controller持有操作成功文案；分别模拟截止Tick和新问题投影。
/// 执行：两个独立控制器进入确认Activity，再推进期限或发布新题。
/// 预期：原反馈在生命周期结束时清除，新问题保留自己的提醒身份。
#[test]
fn island_action_feedback_ends_on_collapse_or_new_alert() {
    for next_alert in [false, true] {
        let mut cx = TestAppContext::single();
        let model = cx.new(|_| IslandModel::default());
        let controller = cx.new(|cx| IslandController::new(model, cx));
        controller.update(&mut cx, |state, cx| {
            state.message = Some(ui::i18n::LocalizedText::new("island.reply_submitted"));
            state.action_feedback("q1".into(), true, cx);
            assert!(state.message.is_some(), "确认期间确实有成功文案");
            if next_alert {
                state.event(
                    1,
                    IslandEvent::OverviewUpdated(IslandOverview {
                        alerts: vec![IslandAlert {
                            id: "q2".into(),
                            kind: IslandAlertKind::Question,
                        }],
                        review_count: 0,
                    }),
                    cx,
                );
                assert_eq!(
                    state.displays.state.alert_id(),
                    Some("q2"),
                    "新问题必须正常弹出"
                );
            } else {
                state.epoch -= Duration::from_millis(2300);
                state.event(1, IslandEvent::Tick, cx);
                assert_eq!(
                    state.displays.state.mode(),
                    runtime::island::IslandMode::Compact,
                    "计时到期收回"
                );
            }
            assert!(state.message.is_none(), "旧成功文案不能跨越确认生命周期");
        });
    }
}

fn proposal_workspace() -> IslandWorkspace {
    let mut overview = rows::sample(2_000_000);
    overview.running.clear();
    overview.idle.clear();
    overview.attention.clear();
    overview.unloaded_projects.clear();
    IslandWorkspace {
        database: "db.sqlite".into(),
        id: "w".into(),
        name: "Workspace".into(),
        projects: vec![super::super::IslandProject {
            source: 1,
            area_id: "a".into(),
            name: "Project".into(),
            root: "/project".into(),
            loaded: true,
            cached_overview: overview.clone(),
            overview,
            approvals: vec![],
            questions: vec![],
            proposals: vec![super::super::IslandProposal {
                id: 7,
                area_id: "a".into(),
                agent: "Ada".into(),
                title: "下一步".into(),
                task: "去做".into(),
                why: "因为空着".into(),
                block: None,
                creates_card: false,
            }],
        }],
    }
}

/// 场景：岛上只有一条幕僚提议，没有审批、提问或成果。
/// 数据：已加载项目带着提议 7。控制器没有注册宿主。
/// 执行：刷新后看模式和徽标，展开，再手动切回运行页并再次刷新；然后对未注册宿主派发和忽略。
/// 预期：不进 Alert；徽标计 1；展开落在待办；手动切回运行页后刷新不抢回去；两次动作都提示失效且不记为已处理。
#[test]
fn island_proposals_open_pending_without_an_alert_and_expire_without_a_host() {
    let mut cx = TestAppContext::single();
    let model = cx.new(|_| {
        let mut model = IslandModel::default();
        model.workspaces = vec![proposal_workspace()];
        model
    });
    let controller = cx.new(|cx| IslandController::new(model, cx));
    controller.update(&mut cx, |state, cx| {
        assert_ne!(
            state.displays.state.mode(),
            runtime::island::IslandMode::Alert,
            "只有提议不能主动弹出"
        );
        assert!(
            state.displays.state.alert_id().is_none(),
            "提议不能占提醒身份"
        );
        assert_eq!(
            counts(&state.visible_workspaces(cx)),
            (0, 1),
            "待办徽标要算上这条提议"
        );
        state.event(1, IslandEvent::Expand, cx);
        assert_eq!(
            state.displays.state.mode(),
            runtime::island::IslandMode::Expanded,
            "展开后停在展开态"
        );
        assert_eq!(
            state.displays.state.tab(),
            runtime::island::IslandTab::Pending,
            "只有提议时展开应落在需要你处理"
        );
        state.event(
            1,
            IslandEvent::SelectTab(runtime::island::IslandTab::Running),
            cx,
        );
        state.refresh(cx);
        assert_eq!(
            state.displays.state.tab(),
            runtime::island::IslandTab::Running,
            "用户切回运行页后，刷新不能再抢回待办"
        );
        let route = super::super::actions::IslandTarget {
            source: 99,
            workspace: "w".into(),
            area: "a".into(),
        };
        state.dispatch_proposal(route.clone(), 7, cx);
        assert_eq!(
            state.message.as_ref(),
            Some(&ui::i18n::LocalizedText::new("island.expired")),
            "没有项目会话时派发应提示失效"
        );
        assert!(
            state.resolved_proposals.is_empty(),
            "失效的派发不能把提议标成已处理"
        );
        state.dismiss_proposal(route, 7, cx);
        assert_eq!(
            state.message.as_ref(),
            Some(&ui::i18n::LocalizedText::new("island.expired")),
            "没有项目会话时忽略也应提示失效"
        );
        assert!(
            state.resolved_proposals.is_empty(),
            "失效的忽略不能把提议标成已处理"
        );
        assert_eq!(
            counts(&state.visible_workspaces(cx)),
            (0, 1),
            "没处理成的提议仍留在待办"
        );
    });
}
