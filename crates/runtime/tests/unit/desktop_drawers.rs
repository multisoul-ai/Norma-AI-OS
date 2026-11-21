use super::*;

/// 标签操作相对于菜单目标计算，失效的目标不能关闭其他标签。
///
/// 数据构造：A、B、C 按显示顺序排列，菜单指向 B。
/// 执行过程：分别计算当前、其他、右侧、全部及目标消失后的范围。
/// 预期结果：得到四个精确集合；无邻居和失效目标均返回空集合。
#[test]
fn tab_close_scopes_follow_menu_target() {
    let ordered = ["a", "b", "c"];
    use ContentTabCloseScope::*;
    for (scope, expected) in [
        (Current, &["b"][..]),
        (Others, &["a", "c"]),
        (Right, &["c"]),
        (All, &["a", "b", "c"]),
    ] {
        assert_eq!(
            scope.targets(&ordered, "b"),
            expected,
            "应按目标 B 计算 {scope:?}"
        );
        assert!(
            scope.targets(&ordered, "gone").is_empty(),
            "目标失效后不能执行 {scope:?}"
        );
    }
    assert!(Right.targets(&ordered, "c").is_empty(), "末位无右侧标签");
    assert!(
        Others.targets(&["a"], "a").is_empty(),
        "仅有一个标签时没有其他标签"
    );
}

/// 搜索应允许用来源区分同名标签，并匹配网页域名。
///
/// 数据构造：两个同名文件、一个网页，来源分别不同。
/// 执行过程：按大小写不同的名称、路径、域名和无关词搜索。
/// 预期结果：匹配名称、指定路径和域名；另一同名文件及无关查询不匹配。
#[test]
fn tab_search_matches_name_and_source() {
    assert!(
        content_tab_matches(" README ", "readme.md", "docs/a/readme.md"),
        "忽略首尾空格及大小写"
    );
    assert!(
        content_tab_matches("docs/a", "readme.md", "docs/a/readme.md"),
        "应匹配文件路径"
    );
    assert!(
        !content_tab_matches("docs/a", "readme.md", "docs/b/readme.md"),
        "同名文件也必须按来源区分"
    );
    assert!(
        content_tab_matches("site.test", "网页", "https://site.test/docs"),
        "应匹配域名"
    );
    assert!(
        !content_tab_matches("任意项", "网页", "https://site.test/docs"),
        "无关搜索无结果"
    );
    assert!(
        content_tab_matches("", "网页", "https://site.test/docs"),
        "空查询列出全部标签"
    );
}

/// 新开工作空间时先呈现完整画布，右侧内容由用户按需展开。
///
/// 数据构造：使用未经过任何用户操作的抽屉默认状态。
/// 执行过程：读取初始状态，再触发一次右栏切换和一次再切换。
/// 预期结果：初始收起；第一次展开；第二次收起，没有残留的左栏状态。
#[test]
fn right_drawer_starts_collapsed_and_toggles() {
    let initial = DesktopDrawerState::default();
    assert!(!initial.right_open, "右侧内容默认应收起");
    let opened = route_drawer(initial, DesktopDrawerEvent::ToggleRight);
    assert!(opened.right_open, "点击入口后右栏应展开");
    let closed = route_drawer(opened, DesktopDrawerEvent::ToggleRight);
    assert!(!closed.right_open, "再次点击应收起");
}

/// 语音对话和后台产物出现时，已收起的抽屉不得自行弹开；只有用户显式打开产物才展开。
///
/// 数据构造：右抽屉收起。
/// 执行过程：触发语音、后台产物通知，再由用户打开产物。
/// 预期结果：前两步保持收起；打开产物后展开。
#[test]
fn voice_and_background_artifact_do_not_force_the_drawer_open() {
    let mut state = DesktopDrawerState::default();
    state = route_drawer(state, DesktopDrawerEvent::VoiceTriggered);
    state = route_drawer(state, DesktopDrawerEvent::ArtifactReady);
    assert!(!state.right_open, "语音和后台产物不能强制打开右栏");
    state = route_drawer(state, DesktopDrawerEvent::OpenArtifact);
    assert!(state.right_open, "用户打开产物后右栏应展开");
}

/// 拖拽右栏内沿：向左拖变宽，夹在最小/最大宽度之间；拖到不足最小宽度则收起。
///
/// 数据构造：初始宽 400、按下 x=1000，最小 320、最大 700。
/// 执行过程：向左拖 100、向左拖 900、向右拖 200。
/// 预期结果：宽 500 且打开；夹到 700 且打开；请求宽 200 < 320 夹到 320 且关闭。
#[test]
fn resizing_right_drawer_clamps_and_collapses() {
    let wider = resize_drawer(400.0, 1000.0, 900.0, 320.0, 700.0);
    assert_eq!(
        (wider.width, wider.open),
        (500.0, true),
        "向左拖 100 应加宽 100"
    );
    let capped = resize_drawer(400.0, 1000.0, 100.0, 320.0, 700.0);
    assert_eq!(
        (capped.width, capped.open),
        (700.0, true),
        "超过上限应夹到最大宽度"
    );
    let collapsed = resize_drawer(400.0, 1000.0, 1200.0, 320.0, 700.0);
    assert_eq!(
        (collapsed.width, collapsed.open),
        (320.0, false),
        "不足最小宽度应收起并保持最小宽度"
    );
}
