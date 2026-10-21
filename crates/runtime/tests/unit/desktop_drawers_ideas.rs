use super::*;

/// 想法并入运行概览：ToggleIdeas 是“带我到想法栏”，只会进入或停留在概览，不再有独立视图。
///
/// 数据构造：左栏收起、停在对话的默认状态；另一份状态停在运行概览。
/// 执行过程：对两份状态各发一次 ToggleIdeas，再对概览状态重复一次。
/// 预期结果：
///   - 正断言：从对话触发会展开左栏并显示运行概览；已在概览时保持概览（可重复触发）。
///   - 负断言：重复触发不会回到对话，也不会收起左栏。
#[test]
fn toggle_ideas_reveals_the_overview_and_stays_there() {
    let opened = route_drawer(
        DesktopDrawerState::default(),
        DesktopDrawerEvent::ToggleIdeas,
    );
    assert!(opened.left_open, "触发想法时收起的左栏应先展开");
    assert_eq!(
        opened.left_view,
        DesktopLeftView::Overview,
        "想法栏在概览里"
    );
    let again = route_drawer(opened, DesktopDrawerEvent::ToggleIdeas);
    assert_eq!(
        again.left_view,
        DesktopLeftView::Overview,
        "重复触发不应回到对话"
    );
    assert!(again.left_open, "重复触发不应收起左栏");
}

/// 悬浮的运行概览里也有想法栏，触发想法不把它收回抽屉。
///
/// 数据构造：概览已弹出悬浮并偏移 (40, 20)，左栏停在对话。
/// 预期结果：
///   - 正断言：悬浮保持，偏移不变。
///   - 负断言：左栏不会被切到概览（概览正悬浮在画布上）。
#[test]
fn toggle_ideas_leaves_floating_overview_alone() {
    let mut state = DesktopDrawerState {
        left_open: true,
        ..DesktopDrawerState::default()
    };
    state = route_drawer(state, DesktopDrawerEvent::ToggleOverviewPin);
    state.overview_offset = OverviewOffset { x: 40, y: 20 };
    let next = route_drawer(state, DesktopDrawerEvent::ToggleIdeas);
    assert!(next.overview_floating, "不应把悬浮概览收回抽屉");
    assert_eq!(
        next.overview_offset,
        OverviewOffset { x: 40, y: 20 },
        "不应重置悬浮偏移"
    );
    assert_eq!(next.left_view, DesktopLeftView::Chat, "左栏保持对话");
}

/// 旧版本保存过“想法”视图偏好：升级后落到运行概览，而不是对话或报错。
///
/// 预期结果：
///   - 正断言：偏好与完整布局 JSON 里的 "ideas" 都还原为运行概览。
///   - 负断言：未知视图名仍回退到对话。
#[test]
fn legacy_ideas_preference_maps_to_overview() {
    assert_eq!(
        left_view_from_preference(Some(r#"{"left_view":"ideas"}"#)),
        DesktopLeftView::Overview,
        "旧的想法偏好应落到概览"
    );
    assert_eq!(
        overview_layout_from_preference(Some(
            r#"{"floating":false,"left_view":"ideas","offset_x":0,"offset_y":0}"#
        ))
        .left_view,
        DesktopLeftView::Overview,
        "旧的完整布局也应落到概览"
    );
    assert_eq!(
        left_view_from_preference(Some(r#"{"left_view":"nonsense"}"#)),
        DesktopLeftView::Chat,
        "未知视图名应回退到对话"
    );
}

/// 左栏两个视图按标签顺序排名，滑入方向据此判断。
///
/// 预期结果：
///   - 正断言：对话 0 < 运行概览 1。
///   - 负断言：两个排名不相同。
#[test]
fn left_views_rank_in_tab_order() {
    assert_eq!(
        [
            DesktopLeftView::Chat.rank(),
            DesktopLeftView::Overview.rank()
        ],
        [0, 1],
        "排名应与标签顺序（对话、运行概览）一致"
    );
}
