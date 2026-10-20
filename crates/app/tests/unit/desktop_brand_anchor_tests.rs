use crate::app::MultisoulApp;
use gpui::{px, size, AppContext, TestAppContext};

/// 标题栏的侧栏开关与 logo 是固定锚点：左抽屉展开/收起时 x 不变，logo 始终可见，
/// “Norma” 字样只在展开时出现，面包屑在收起时紧贴 logo。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个空间 Studio，不创建 PTY 或 Agent；窗口 1280×800（wide，宽 ≥ 780）。
///   开关 32px 宽，左边距 82px（避开交通灯）→ 开关占 82..114。
///   开关与 logo 间距 10px → logo 左边 = 82 + 32 + 10 = 124px，logo 边长 22px → 124..146。
///   收起时面包屑起点 = logo 右边 146 + 12px 间距 = 158px，容差 1px 覆盖像素取整。
///
/// 执行过程：
///   1. 展开左抽屉并绘制，记录关闭按钮与 logo 的边界，确认“Norma”字样存在。
///   2. 收起左抽屉并绘制，记录展开按钮与 logo 的边界，确认字样消失、面包屑贴近 logo。
///   3. 缩至 640px 窄窗，确认 logo 仍然可见且面包屑不压住 logo。
///
/// 预期结果：
///   - 正断言：两种状态下开关 left、logo left 完全一致；收起后面包屑左边 ≈ 158；窄窗 logo 仍存在。
///   - 负断言：收起后不存在“Norma”字样；窄窗面包屑左边不得落在 logo 右边之内。
#[test]
fn titlebar_toggle_and_logo_stay_anchored_across_left_drawer_states() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离数据库");
    let workspace = store
        .desktop_workspace_store()
        .create("Studio")
        .expect("应创建 Studio");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
        app.desktop_workspace.drawers.left_open = true;
        app.desktop_workspace.drawer_motion.settle_left(true);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| app.finish_desktop_zoom(window, cx));
    cx.update(|window, cx| window.draw(cx).clear());

    let open_toggle = cx
        .debug_bounds("desktop-close-left-drawer")
        .expect("展开态必须有收起开关");
    let open_logo = cx
        .debug_bounds("desktop-brand-logo")
        .expect("展开态必须显示 logo");
    assert!(
        cx.debug_bounds("desktop-brand-wordmark").is_some(),
        "展开态必须显示 Norma 字样"
    );
    assert!(
        (open_logo.left() - open_toggle.right() - px(10.0)).abs() <= px(1.0),
        "logo 必须紧贴开关右侧 10px，不能被开关覆盖或漂移：开关 {open_toggle:?}，logo {open_logo:?}"
    );

    host.update_in(cx, |app, _, cx| {
        app.desktop_workspace.drawers.left_open = false;
        app.desktop_workspace.drawer_motion.settle_left(false);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());

    let closed_toggle = cx
        .debug_bounds("desktop-open-left-drawer")
        .expect("收起态必须有展开开关");
    let closed_logo = cx
        .debug_bounds("desktop-brand-logo")
        .expect("收起态也必须显示 logo");
    assert_eq!(
        closed_toggle.left(),
        open_toggle.left(),
        "开关的 x 在展开与收起之间不能移动"
    );
    assert_eq!(
        closed_logo.left(),
        open_logo.left(),
        "logo 的 x 在展开与收起之间不能移动"
    );
    assert!(
        cx.debug_bounds("desktop-brand-wordmark").is_none(),
        "收起后不应再显示 Norma 字样"
    );
    let title = cx
        .debug_bounds("workspace-topbar-title")
        .expect("收起态应显示面包屑");
    assert!(
        (title.left() - closed_logo.right() - px(12.0)).abs() <= px(1.0),
        "收起后面包屑应紧贴 logo 右侧 12px：logo {closed_logo:?}，标题 {title:?}"
    );

    cx.simulate_resize(size(px(640.0), px(800.0)));
    cx.update(|window, cx| window.draw(cx).clear());
    let narrow_logo = cx
        .debug_bounds("desktop-brand-logo")
        .expect("窄窗收起态 logo 仍须可见");
    let narrow_title = cx
        .debug_bounds("workspace-topbar-title")
        .expect("窄窗应保留面包屑");
    assert!(
        narrow_title.left() >= narrow_logo.right(),
        "窄窗面包屑不能压住 logo：logo {narrow_logo:?}，标题 {narrow_title:?}"
    );
}
