use super::*;
use crate::app::AppScreen;
use gpui::{size, Entity, Render, TestAppContext};
use std::time::Duration;

struct DirectoryView(Entity<MultisoulApp>);

impl Render for DirectoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().relative().size_full().child(
            self.0
                .update(cx, |app, cx| app.render_desktop_launchpad(window, cx)),
        )
    }
}

#[derive(Clone, Copy, Debug)]
enum CardCache {
    /// 上次停留时按当前抽屉布局捕获。
    Fresh,
    /// 上次停留时右抽屉关闭，之后才打开。
    Stale,
    /// 启动后从未进入该空间。
    Missing,
}

/// 右抽屉打开时从目录直接进入/退出项目，外壳随镜头连续淡出/淡入，落地不残留。
///
/// 数据构造：窗口 1280×800，1 个工作空间含 1/2/5 个项目，右抽屉完全展开，开启动效；
///   普通/首页两种目录 × 项目卡片缓存新鲜/过期（抽屉关闭时捕获）/缺失。
/// 执行过程：经真实入口发起目录→项目镜头 → 逐帧推进到静止，记录外壳不透明度与工作空间画布帧
///   → 渲染静止帧；再发起项目→目录镜头逐帧记录。
/// 预期结果：进入时外壳 1→0 无跳变、落地工作空间画布至少占满窗口宽，普通目录静止帧标题栏高 0；
///   退出时外壳 0→1 无跳变。
#[test]
fn desktop_directory_project_zoom_lands_without_workspace_shell() {
    for home in [false, true] {
        for count in [1, 2, 5] {
            for cache in [CardCache::Fresh, CardCache::Stale, CardCache::Missing] {
                check_directory_project_zoom(home, count, cache);
            }
        }
    }
}

fn check_directory_project_zoom(home: bool, count: usize, cache: CardCache) {
    let case = format!("home={home} projects={count} cache={cache:?}");
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let workspaces = store.desktop_workspace_store();
    let workspace = workspaces.create("Drawer").expect("应创建空间");
    let areas: Vec<_> = (0..count)
        .map(|index| {
            let root = temp.path().join(format!("project-{index}"));
            std::fs::create_dir_all(&root).expect("应创建项目目录");
            workspaces
                .add_project(&workspace.id, &root)
                .expect("应添加项目")
        })
        .collect();
    let mut test = TestAppContext::single();
    let (view, cx) = test.add_window_view(|_, cx| {
        DirectoryView(cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)))
    });
    let host = view.update(cx, |view, _| view.0.clone());
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    let target = areas[count - 1].id.clone();
    let id = workspace.id.clone();
    let full = CanvasRect::new(0.0, 0.0, 1280.0, 800.0);
    let run = |reverse: bool, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            host.update(cx, |app, cx| {
                app.begin_desktop_project_directory_zoom(&id, &target, reverse, window, cx);
                let viewport = window.viewport_size();
                let opacity = |app: &MultisoulApp| {
                    app.desktop_directory_chrome_opacity(&id, viewport)
                        .expect("外壳应参与镜头")
                };
                let mut series = vec![opacity(app)];
                loop {
                    let transition = app.desktop_workspace.transition.as_mut().expect("镜头");
                    if !transition.is_active() {
                        break;
                    }
                    transition.tick(Duration::from_millis(16));
                    series.push(opacity(app));
                }
                cx.notify();
                (series, app.desktop_scene_frame(full, false))
            })
        })
    };
    cx.update(|window, cx| {
        host.update(cx, |app, cx| {
            app.motion.reduce_motion = false;
            app.desktop_workspace.home_directory = home;
            app.desktop_workspace.workspaces = vec![workspace.clone()];
            app.desktop_workspace.active_id = Some(workspace.id.clone());
            app.desktop_workspace.areas = areas.clone();
            if matches!(cache, CardCache::Stale) {
                app.screen = AppScreen::DesktopWorkspace;
                app.desktop_project_overview_rect(&id, &target, window)
                    .expect("应捕获无抽屉时的项目卡片");
            }
            app.desktop_workspace.drawers.right_open = true;
            app.desktop_workspace.drawer_motion.retarget(false, true);
            while app.desktop_workspace.drawer_motion.is_active() {
                app.desktop_workspace
                    .drawer_motion
                    .tick(Duration::from_millis(16), false);
            }
            if matches!(cache, CardCache::Fresh) {
                app.screen = AppScreen::DesktopWorkspace;
                app.desktop_project_overview_rect(&id, &target, window)
                    .expect("应捕获抽屉间的项目卡片");
            }
            app.screen = AppScreen::Launchpad;
            assert!(
                app.desktop_workspace_expanded_rect(window.viewport_size())
                    .size
                    .x
                    < 1280.0,
                "右抽屉应收窄工作空间画布"
            );
            cx.notify();
        })
    });
    let (series, frame) = run(false, cx);
    assert_fades(&series, 1.0, 0.0, home);
    assert!(
        frame.size.x >= full.size.x - 0.5,
        "落地时工作空间画布应放大到至少整窗宽（{case}），实际 {frame:?}"
    );
    if !home {
        cx.update(|window, cx| window.draw(cx).clear());
        let header = cx
            .debug_bounds("desktop-workspace-header-0")
            .expect("镜头期间目录外壳仍在渲染");
        assert!(
            f32::from(header.size.height) < 0.5,
            "静止帧外壳应完全淡出（{case}），实际标题栏高 {:?}",
            header.size.height
        );
    }
    cx.update(|_, cx| host.update(cx, |app, _| app.desktop_workspace.transition = None));
    let (series, _) = run(true, cx);
    assert_fades(&series, 0.0, 1.0, home);
}

/// 右抽屉打开时进入/退出工作空间，目录外壳随镜头连续淡出/淡入，落地帧不残留。
///
/// 数据构造：1 个工作空间，窗口 1280×800，右抽屉已完全展开（抽屉间画布窄于整窗）；开启动效。
/// 执行过程：两种目录分别经真实入口发起进入镜头 → 逐帧推进到静止并记录外壳不透明度
///   → 渲染静止帧读取外壳标题栏；再发起退出镜头逐帧记录。
/// 预期结果：进入镜头落在抽屉间画布，外壳 1→0 无跳变，静止帧标题栏高度为 0；
///   退出镜头外壳 0→1 无跳变。
#[test]
fn desktop_workspace_shell_fades_out_at_drawer_landing() {
    for home in [false, true] {
        let temp = tempfile::tempdir().expect("应创建隔离目录");
        let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
        let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
        let workspace = store
            .desktop_workspace_store()
            .create("Drawer")
            .expect("应创建空间");
        let mut test = TestAppContext::single();
        let (view, cx) = test.add_window_view(|_, cx| {
            DirectoryView(cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)))
        });
        let host = view.update(cx, |view, _| view.0.clone());
        cx.simulate_resize(size(px(1280.0), px(800.0)));
        cx.run_until_parked();
        let id = workspace.id.clone();
        let run = |reverse: bool, cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| {
                host.update(cx, |app, cx| {
                    app.begin_desktop_workspace_zoom(&id, reverse, window, cx);
                    let viewport = window.viewport_size();
                    let mut series = vec![app
                        .desktop_directory_chrome_opacity(&id, viewport)
                        .expect("外壳应参与镜头")];
                    while app
                        .desktop_workspace
                        .transition
                        .as_ref()
                        .expect("应发起镜头")
                        .is_active()
                    {
                        let transition = app.desktop_workspace.transition.as_mut().expect("镜头");
                        transition.tick(Duration::from_millis(16));
                        series.push(
                            app.desktop_directory_chrome_opacity(&id, viewport)
                                .expect("外壳应参与镜头"),
                        );
                    }
                    let landed = app
                        .desktop_workspace
                        .transition
                        .as_ref()
                        .expect("镜头")
                        .rect();
                    (
                        series,
                        landed,
                        app.desktop_workspace_expanded_rect(viewport),
                    )
                })
            })
        };
        cx.update(|_, cx| {
            host.update(cx, |app, cx| {
                app.screen = AppScreen::Launchpad;
                app.motion.reduce_motion = false;
                app.desktop_workspace.home_directory = home;
                app.desktop_workspace.workspaces = vec![workspace.clone()];
                app.desktop_workspace.active_id = Some(workspace.id.clone());
                app.desktop_workspace.drawers.right_open = true;
                app.desktop_workspace.drawer_motion.retarget(false, true);
                while app.desktop_workspace.drawer_motion.is_active() {
                    app.desktop_workspace
                        .drawer_motion
                        .tick(Duration::from_millis(16), false);
                }
                cx.notify();
            })
        });
        let (series, landed, expanded) = run(false, cx);
        assert!(expanded.size.x < 1280.0, "右抽屉应收窄工作空间画布");
        assert_eq!(landed, expanded, "进入镜头应落在抽屉间画布（home={home}）");
        assert_fades(&series, 1.0, 0.0, home);
        if !home {
            cx.update(|window, cx| window.draw(cx).clear());
            let header = cx
                .debug_bounds("desktop-workspace-header-0")
                .expect("镜头期间目录外壳仍在渲染");
            assert!(
                f32::from(header.size.height) < 0.5,
                "静止帧外壳应完全淡出，实际标题栏高 {:?}",
                header.size.height
            );
        }
        cx.update(|_, cx| host.update(cx, |app, _| app.desktop_workspace.transition = None));
        let (series, _, _) = run(true, cx);
        assert_fades(&series, 0.0, 1.0, home);
    }
}

/// 外壳不透明度序列从 `from` 连续变化到 `to`：首尾吻合且相邻帧不跳变。
fn assert_fades(series: &[f32], from: f32, to: f32, home: bool) {
    let first = series.first().copied().unwrap_or(f32::NAN);
    let last = series.last().copied().unwrap_or(f32::NAN);
    assert!(
        (first - from).abs() < 0.05,
        "起点应为 {from}（home={home}），序列 {series:?}"
    );
    assert!(
        (last - to).abs() < 0.001,
        "终点应为 {to}（home={home}），序列 {series:?}"
    );
    let jump = series
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        jump < 0.35,
        "相邻帧不得跳变（home={home}），最大差 {jump}，序列 {series:?}"
    );
}
