use super::*;

/// 新开工作空间时先呈现完整画布，Norma 对话与右侧内容由用户按需展开。
///
/// 数据构造：使用未经过任何用户操作的抽屉默认状态。
/// 执行过程：读取初始开合状态，再触发一次左栏切换。
/// 预期结果：初始两侧均收起、概览折叠；用户点击后左栏仍可展开。
#[test]
fn desktop_drawers_start_collapsed_and_left_can_be_opened() {
    let initial = DesktopDrawerState::default();
    assert!(!initial.left_open, "左侧 Norma 对话默认应收起");
    assert!(!initial.right_open, "右侧内容默认应收起");
    assert!(!initial.overview_expanded, "运行概览默认应折叠");

    let opened = route_drawer(initial, DesktopDrawerEvent::ToggleLeft);
    assert!(opened.left_open, "用户点击左侧入口后应展开对话");
    assert!(!opened.right_open, "展开左栏不得连带打开右栏");
}

/// 语音对话和后台产物出现时，已收起的抽屉不得自行弹开。
///
/// 数据构造：左、右抽屉均已收起，悬浮概览也已折叠。
/// 执行过程：触发语音、后台产物通知，再由用户显式打开产物和概览。
/// 预期结果：前两步不改开合状态；用户操作才打开右栏和概览，左栏仍收起。
#[test]
fn voice_and_background_artifact_do_not_force_drawers_open() {
    let mut state = DesktopDrawerState {
        left_open: false,
        right_open: false,
        overview_expanded: false,
    };
    state = route_drawer(state, DesktopDrawerEvent::VoiceTriggered);
    state = route_drawer(state, DesktopDrawerEvent::ArtifactReady);
    assert!(!state.left_open, "语音触发不能强制打开左栏");
    assert!(!state.right_open, "后台产物不能强制打开右栏");
    assert!(!state.overview_expanded, "后台产物不能强制展开概览");

    state = route_drawer(state, DesktopDrawerEvent::OpenArtifact);
    state = route_drawer(state, DesktopDrawerEvent::ToggleOverview);
    assert!(state.right_open, "用户打开产物后右栏应展开");
    assert!(state.overview_expanded, "用户点击后概览应展开");
    assert!(!state.left_open, "打开产物不能顺带打开左栏");
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
