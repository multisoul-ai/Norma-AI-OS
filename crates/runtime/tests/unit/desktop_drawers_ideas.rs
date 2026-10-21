use super::*;

/// 想法视图与对话、运行概览三者互相切换，且必要时自动展开收起的左栏。
///
/// 数据构造：左栏收起、停在对话的默认状态。
/// 执行过程：ToggleIdeas → 再次 ToggleIdeas；进入概览后 ToggleIdeas；停在想法时 ToggleRunOverview。
/// 预期结果：
///   - 正断言：第一次 ToggleIdeas 展开左栏并显示想法；再次回到对话；从概览可直达想法；从想法切运行概览得到概览。
///   - 负断言：从想法切运行概览不能落回对话（它是“到概览”，不是“关闭想法”）。
#[test]
fn ideas_view_toggles_between_chat_and_overview() {
    let opened = route_drawer(
        DesktopDrawerState::default(),
        DesktopDrawerEvent::ToggleIdeas,
    );
    assert!(opened.left_open, "切到想法时收起的左栏应先展开");
    assert_eq!(
        opened.left_view,
        DesktopLeftView::Ideas,
        "ToggleIdeas 应显示想法"
    );
    let back = route_drawer(opened, DesktopDrawerEvent::ToggleIdeas);
    assert_eq!(
        back.left_view,
        DesktopLeftView::Chat,
        "在想法里再次切换应回到对话"
    );
    assert!(back.left_open, "回到对话不应收起左栏");

    let overview = route_drawer(
        DesktopDrawerState::default(),
        DesktopDrawerEvent::ToggleRunOverview,
    );
    let to_ideas = route_drawer(overview, DesktopDrawerEvent::ToggleIdeas);
    assert_eq!(
        to_ideas.left_view,
        DesktopLeftView::Ideas,
        "从概览应能直达想法"
    );
    let to_overview = route_drawer(to_ideas, DesktopDrawerEvent::ToggleRunOverview);
    assert_eq!(
        to_overview.left_view,
        DesktopLeftView::Overview,
        "从想法切运行概览应得到概览"
    );
    assert_ne!(
        to_overview.left_view,
        DesktopLeftView::Chat,
        "不能把“到概览”误判成关闭概览后回对话"
    );
}

/// 悬浮的运行概览不受想法视图影响。
///
/// 数据构造：概览已弹出悬浮并偏移 (40, 20)，左栏停在对话。
/// 执行过程：ToggleIdeas；SelectLeftView(Ideas)。
/// 预期结果：
///   - 正断言：左栏显示想法。
///   - 负断言：概览仍然悬浮，偏移不变；选择想法不会把概览收回抽屉。
#[test]
fn ideas_selection_leaves_floating_overview_alone() {
    let mut state = DesktopDrawerState {
        left_open: true,
        ..DesktopDrawerState::default()
    };
    state = route_drawer(state, DesktopDrawerEvent::ToggleOverviewPin);
    state.overview_offset = OverviewOffset { x: 40, y: 20 };
    for event in [
        DesktopDrawerEvent::ToggleIdeas,
        DesktopDrawerEvent::SelectLeftView(DesktopLeftView::Ideas),
    ] {
        let next = route_drawer(state, event);
        assert_eq!(
            next.left_view,
            DesktopLeftView::Ideas,
            "{event:?} 后左栏应显示想法"
        );
        assert!(next.overview_floating, "{event:?} 不应把悬浮概览收回抽屉");
        assert_eq!(
            next.overview_offset,
            OverviewOffset { x: 40, y: 20 },
            "{event:?} 不应重置悬浮偏移"
        );
    }
}

/// Esc 在想法视图里回到对话，且不收起左栏。
///
/// 预期结果：
///   - 正断言：两种 Esc 路径（事件与焦点路由）都把想法切回对话。
///   - 负断言：已在对话时 Esc 不改变视图；路由结果标记为“抽屉回到对话”而不是忽略。
#[test]
fn escape_returns_ideas_to_chat_without_closing_drawer() {
    let ideas = route_drawer(
        DesktopDrawerState::default(),
        DesktopDrawerEvent::ToggleIdeas,
    );
    let by_event = route_drawer(ideas, DesktopDrawerEvent::EscapeOverview);
    assert_eq!(
        by_event.left_view,
        DesktopLeftView::Chat,
        "Esc 事件应让想法回到对话"
    );
    assert!(by_event.left_open, "Esc 回对话不应收起左栏");
    let (by_focus, outcome) = route_overview_escape(ideas, false);
    assert_eq!(
        by_focus.left_view,
        DesktopLeftView::Chat,
        "焦点路由的 Esc 也应回对话"
    );
    assert_eq!(
        outcome,
        OverviewEscape::ReturnDrawerToChat,
        "应标记为抽屉回到对话"
    );
    let (still_chat, outcome) = route_overview_escape(by_focus, false);
    assert_eq!(
        (still_chat.left_view, outcome),
        (DesktopLeftView::Chat, OverviewEscape::Ignore),
        "已在对话时 Esc 应被忽略"
    );
}

/// 想法视图的偏好可保存、还原，损坏值回退到对话。
///
/// 预期结果：
///   - 正断言：偏好 JSON 与完整布局 JSON 都能还原想法；还原时收起的左栏先展开。
///   - 负断言：未知视图名回退到对话，而不是想法或概览。
#[test]
fn ideas_view_preference_round_trips_and_unknown_falls_back_to_chat() {
    let saved = left_view_preference_json(DesktopLeftView::Ideas);
    assert_eq!(
        left_view_from_preference(Some(saved)),
        DesktopLeftView::Ideas,
        "想法偏好应能还原"
    );
    let state = DesktopDrawerState {
        left_view: DesktopLeftView::Ideas,
        ..DesktopDrawerState::default()
    };
    let layout = overview_layout_json(&state);
    assert_eq!(
        overview_layout_from_preference(Some(&layout)).left_view,
        DesktopLeftView::Ideas,
        "完整布局 JSON 应保留想法视图"
    );
    let restored = restore_left_view(DesktopDrawerState::default(), DesktopLeftView::Ideas);
    assert!(restored.left_open, "还原想法视图时收起的左栏应先展开");
    assert_eq!(restored.left_view, DesktopLeftView::Ideas, "应还原到想法");
    assert_eq!(
        left_view_from_preference(Some(r#"{"left_view":"nonsense"}"#)),
        DesktopLeftView::Chat,
        "未知视图名应回退到对话"
    );
}

/// 左栏三个视图按标签顺序排名，滑入方向据此判断：向右（排名变大）或向左。
///
/// 预期结果：
///   - 正断言：对话 < 想法 < 运行概览，排名依次 0、1、2。
///   - 负断言：三个排名互不相同，不会出现两个视图同方向无法区分。
#[test]
fn left_views_rank_in_tab_order() {
    assert_eq!(
        [
            DesktopLeftView::Chat.rank(),
            DesktopLeftView::Ideas.rank(),
            DesktopLeftView::Overview.rank()
        ],
        [0, 1, 2],
        "排名应与标签顺序（对话、想法、运行概览）一致"
    );
}
