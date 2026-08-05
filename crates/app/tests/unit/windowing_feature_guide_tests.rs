use super::*;

/// WN-4：生产窗口必须以 gpui-component Root 为第一层，供 Settings 原生输入安全绘制。
///
/// 数据构造（含关键数值的推导过程）：
///   production windows = 1 个（通过 `open_multisoul_window` 创建 Launchpad）
///   expected component roots = 1 个
///   expected raw roots = 0；focused app handles = 1（原生菜单 action 依赖 focused dispatch node）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 初始化 gpui-component，并调用生产窗口工厂创建 Launchpad 窗口。
///   2. 从真实 Window 查询第一层是否为 `ui::component::Root`。
///   3. 同时查询第一层是否错误地直接暴露 `MultisoulApp`，并检查 app focus handle。
///   4. 模拟 macOS 菜单接管焦点，使 active window 暂时为空。
///   5. 查询全局 action availability，再派发 `OpenFeatureGuide`。
///
/// 预期结果：
///   - 正断言：生产窗口第一层必须存在 gpui-component Root，Input 绘制才能读取上下文。
///   - 正断言：MultisoulApp focus handle 必须已聚焦，Help 等菜单 action 才能投递。
///   - 正断言：OpenFeatureGuide 必须对原生菜单报告 available，不能被 macOS 禁用。
///   - 正断言：即使菜单使窗口暂时失活，OpenFeatureGuide 后导览也必须可见。
///   - 负断言：MultisoulApp 不得继续直接成为窗口第一层，否则点击 Input 会 panic。
#[test]
fn test_production_window_wraps_multisoul_app_in_component_root() {
    let database_root = tempfile::tempdir().expect("窗口 Root 测试应创建隔离数据库目录");
    let database_paths = storage::DatabasePaths::isolated_test(database_root.path());
    let mut test_cx = gpui::TestAppContext::single();
    let opened_window = test_cx.update(|cx| {
        ui::init_components(cx);
        crate::register_global_actions(cx);
        open_multisoul_window(
            InitialWindowState::Launchpad,
            database_paths.clone(),
            None,
            cx,
        )
        .expect("生产窗口 Root 回归测试应创建 Launchpad 窗口")
    });
    let app = opened_window.app.clone();
    test_cx.run_until_parked();
    let (has_component_root, has_raw_app_root, app_root_focused) = opened_window
        .root_handle
        .update(&mut test_cx, |_root, window, cx| {
            (
                window.root::<ui::component::Root>().flatten().is_some(),
                window.root::<MultisoulApp>().flatten().is_some(),
                app.read(cx).focus_handle.is_focused(window),
            )
        })
        .expect("生产窗口 Root 回归测试应读取窗口第一层");
    let action_available =
        test_cx.update(|cx| cx.is_action_available(&crate::app::OpenFeatureGuide));
    let mut visual =
        gpui::VisualTestContext::from_window(opened_window.root_handle.into(), &test_cx);
    visual.deactivate_window();
    let active_window_during_menu = test_cx.update(|cx| cx.active_window());
    test_cx.update(|cx| cx.dispatch_action(&crate::app::OpenFeatureGuide));
    test_cx.run_until_parked();
    let guide_visible = app.read_with(&test_cx, |app, _cx| app.feature_guide.is_visible());
    assert!(
        has_component_root,
        "生产窗口必须由 gpui-component Root 包裹，否则 Settings Input 绘制会 panic"
    );
    assert!(
        !has_raw_app_root,
        "MultisoulApp 不得直接作为生产窗口第一层，否则组件 Root 上下文缺失"
    );
    assert!(
        app_root_focused,
        "生产窗口创建后必须聚焦 MultisoulApp，否则 Help 等原生菜单 action 无法投递"
    );
    assert!(
        action_available,
        "OpenFeatureGuide 必须对原生菜单报告 available，否则 macOS 会禁用 Help 入口"
    );
    assert!(
        active_window_during_menu.is_none(),
        "回归前置必须模拟 macOS 菜单接管焦点，否则无法覆盖原生 Help 的失活窗口边界"
    );
    assert!(
        guide_visible,
        "窗口因原生菜单暂时失活时，OpenFeatureGuide 仍必须路由到现有窗口并显示导览"
    );
}
