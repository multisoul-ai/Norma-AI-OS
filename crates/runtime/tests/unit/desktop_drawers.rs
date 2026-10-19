use super::*;

/// 新开工作空间时先呈现完整画布，Norma 对话与右侧内容由用户按需展开。
///
/// 数据构造：使用未经过任何用户操作的抽屉默认状态。
/// 执行过程：读取初始开合状态，再触发一次左栏切换。
/// 预期结果：初始两侧均收起、左栏停在对话；用户点击后左栏仍可展开。
#[test]
fn desktop_drawers_start_collapsed_and_left_can_be_opened() {
    let initial = DesktopDrawerState::default();
    assert!(!initial.left_open, "左侧 Norma 对话默认应收起");
    assert!(!initial.right_open, "右侧内容默认应收起");
    assert_eq!(
        initial.left_view,
        DesktopLeftView::Chat,
        "左栏默认应是对话，而不是运行概览"
    );

    let opened = route_drawer(initial, DesktopDrawerEvent::ToggleLeft);
    assert!(opened.left_open, "用户点击左侧入口后应展开对话");
    assert!(!opened.right_open, "展开左栏不得连带打开右栏");
    assert_eq!(
        opened.left_view,
        DesktopLeftView::Chat,
        "只展开左栏时不得改到运行概览"
    );
}

/// 语音对话和后台产物出现时，已收起的抽屉不得自行弹开。
///
/// 数据构造：左、右抽屉均已收起，左栏视图是对话。
/// 执行过程：触发语音、后台产物通知，再由用户显式打开产物。
/// 预期结果：前两步不改开合与视图；打开产物只展开右栏。
#[test]
fn voice_and_background_artifact_do_not_force_drawers_open() {
    let mut state = DesktopDrawerState {
        left_open: false,
        right_open: false,
        left_view: DesktopLeftView::Chat,
    };
    state = route_drawer(state, DesktopDrawerEvent::VoiceTriggered);
    state = route_drawer(state, DesktopDrawerEvent::ArtifactReady);
    assert!(!state.left_open, "语音触发不能强制打开左栏");
    assert!(!state.right_open, "后台产物不能强制打开右栏");
    assert_eq!(
        state.left_view,
        DesktopLeftView::Chat,
        "后台产物不能把左栏切到运行概览"
    );

    state = route_drawer(state, DesktopDrawerEvent::OpenArtifact);
    assert!(state.right_open, "用户打开产物后右栏应展开");
    assert!(!state.left_open, "打开产物不能顺带打开左栏");
    assert_eq!(
        state.left_view,
        DesktopLeftView::Chat,
        "打开产物不能把左栏切到运行概览"
    );
}

/// 切换运行概览会在左栏收起时先展开，再切回对话；Esc 只从概览回到对话。
///
/// 数据构造：左栏收起且当前是对话；偏好 JSON 使用字面量 `{"left_view":"overview"}`。
/// 执行过程：切换概览、再切换一次、对概览按 Esc，并把偏好做一次保存与还原。
/// 预期结果：第一次展开左栏并显示概览；第二次回到对话且左栏仍开着；
///   Esc 回到对话；坏 JSON 与对话偏好都不会被还原成概览。
#[test]
fn toggle_run_overview_opens_collapsed_left_drawer_and_escape_returns_to_chat() {
    let collapsed = DesktopDrawerState::default();
    let overview = route_drawer(collapsed, DesktopDrawerEvent::ToggleRunOverview);
    assert!(overview.left_open, "左栏收起时切到概览应先展开左栏");
    assert_eq!(
        overview.left_view,
        DesktopLeftView::Overview,
        "第一次切换应显示运行概览"
    );
    assert!(!overview.right_open, "切换运行概览不得打开右栏");

    let chat = route_drawer(overview, DesktopDrawerEvent::ToggleRunOverview);
    assert_eq!(chat.left_view, DesktopLeftView::Chat, "再次切换应回到对话");
    assert!(chat.left_open, "切回对话时不应把已经展开的左栏收起");
    assert_ne!(
        chat.left_view,
        DesktopLeftView::Overview,
        "再次切换后不能仍停在运行概览"
    );

    let escaped = route_drawer(
        route_drawer(
            chat,
            DesktopDrawerEvent::SelectLeftView(DesktopLeftView::Overview),
        ),
        DesktopDrawerEvent::EscapeOverview,
    );
    assert_eq!(
        escaped.left_view,
        DesktopLeftView::Chat,
        "焦点在抽屉概览时 Esc 应回到对话"
    );
    assert!(escaped.left_open, "Esc 回对话不应收起左栏");
    let still_chat = route_drawer(escaped, DesktopDrawerEvent::EscapeOverview);
    assert_eq!(
        still_chat.left_view,
        DesktopLeftView::Chat,
        "已经在对话时 Esc 不应再改变视图"
    );

    let saved = left_view_preference_json(DesktopLeftView::Overview);
    assert_eq!(
        left_view_from_preference(Some(saved)),
        DesktopLeftView::Overview,
        "概览偏好应能按原样还原"
    );
    let restored = restore_left_view(DesktopDrawerState::default(), DesktopLeftView::Overview);
    assert!(restored.left_open, "重启后还原概览时，收起的左栏应先展开");
    assert_eq!(
        restored.left_view,
        DesktopLeftView::Overview,
        "重启后应还原运行概览"
    );
    assert_eq!(
        left_view_from_preference(Some("{")),
        DesktopLeftView::Chat,
        "损坏的布局偏好应回退到对话"
    );
    assert_eq!(
        left_view_from_preference(None),
        DesktopLeftView::Chat,
        "没有保存过的工作空间应保持对话"
    );
    assert_ne!(
        left_view_from_preference(Some(r#"{"left_view":"chat"}"#)),
        DesktopLeftView::Overview,
        "对话偏好不能被读成运行概览"
    );
}

/// 场景：用户拖动任一抽屉的内侧边缘，越过最小宽度时抽屉收起。
/// 数据构造：左栏宽 340px、右栏宽 420px，最小分别为 300px、370px。
/// 执行过程：向外扩大、向内缩到阈值、再继续缩过阈值。
/// 预期结果：两侧方向相反；到最小宽度仍展开，低于阈值才收起。
#[test]
fn drawer_resize_obeys_side_direction_and_collapse_threshold() {
    let left = resize_drawer(DesktopDrawerSide::Left, 340.0, 100.0, 140.0, 300.0, 600.0);
    assert_eq!(left.width, 380.0, "左栏内侧边缘向右拖应加宽");
    assert!(left.open, "扩大左栏不能收起");

    let right = resize_drawer(DesktopDrawerSide::Right, 420.0, 800.0, 760.0, 370.0, 600.0);
    assert_eq!(right.width, 460.0, "右栏内侧边缘向左拖应加宽");
    assert!(right.open, "扩大右栏不能收起");

    let edge = resize_drawer(DesktopDrawerSide::Left, 340.0, 100.0, 60.0, 300.0, 600.0);
    assert_eq!(edge.width, 300.0, "最小宽度应仍可停靠");
    assert!(edge.open, "正好到最小宽度不能收起");

    let collapsed = resize_drawer(DesktopDrawerSide::Right, 420.0, 800.0, 851.0, 370.0, 600.0);
    assert_eq!(collapsed.width, 370.0, "收起后应保留可恢复的最小宽度");
    assert!(!collapsed.open, "小于最小宽度应收起右栏");
}
