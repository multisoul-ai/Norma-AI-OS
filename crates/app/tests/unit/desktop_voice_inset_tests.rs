use super::*;
use crate::app::voice_window_inset::{TtsPlaybackEvent, TtsPlaybackSource};
use crate::app::{AppScreen, MultisoulApp};
use gpui::{point, px, size, AppContext, Entity, Modifiers, TestAppContext, VisualTestContext};

struct WorkspaceFixture {
    cx: VisualTestContext,
    host: Entity<MultisoulApp>,
    projects: [Entity<MultisoulApp>; 2],
    area_ids: [String; 2],
    _directory: tempfile::TempDir,
}

impl WorkspaceFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("应创建隔离目录");
        let paths = storage::DatabasePaths::isolated_test(&directory.path().join("data"));
        let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
        let catalog = store.desktop_workspace_store();
        let workspace = catalog.create("Voice inset").expect("应创建工作空间");
        let areas = ["A", "B"].map(|name| {
            let root = directory.path().join(name);
            std::fs::create_dir(&root).expect("应创建空项目目录");
            catalog
                .add_project(&workspace.id, &root)
                .expect("应添加项目")
        });
        let mut test = TestAppContext::single();
        let mut host = None;
        let (_, cx) = test.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone())
            });
            host = Some(entity.clone());
            ui::component::Root::new(entity, window, cx)
        });
        let mut cx = cx.clone();
        let host = host.expect("应创建宿主窗口");
        cx.simulate_resize(size(px(1280.0), px(800.0)));
        cx.run_until_parked();
        let projects = areas.each_ref().map(|_| {
            let project = cx.new(|cx| {
                MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone())
            });
            project.update(&mut cx, |app, cx| {
                app.screen = AppScreen::Canvas;
                app.desktop_host = Some(host.downgrade());
                app.motion.reduce_motion = true;
                cx.notify();
            });
            project
        });
        cx.run_until_parked();
        host.update(&mut cx, |app, cx| {
            app.screen = AppScreen::DesktopWorkspace;
            app.motion.reduce_motion = true;
            app.desktop_workspace.active_id = Some(workspace.id.clone());
            app.desktop_workspace.expanded_area_id = Some(areas[0].id.clone());
            app.desktop_workspace.workspaces = vec![workspace.clone()];
            app.desktop_workspace.areas = areas.to_vec();
            for (area, project) in areas.iter().zip(&projects) {
                app.desktop_workspace
                    .sessions
                    .insert((workspace.id.clone(), area.id.clone()), project.clone());
            }
            cx.notify();
        });
        Self {
            cx,
            host,
            projects,
            area_ids: areas.map(|area| area.id),
            _directory: directory,
        }
    }

    fn draw(&mut self) {
        self.cx.run_until_parked();
        self.cx.update(|window, cx| window.draw(cx).clear());
    }

    fn start(&mut self, app: &Entity<MultisoulApp>) {
        app.update(&mut self.cx, |app, _| {
            app.voice_window_inset_events
                .push(TtsPlaybackSource::Doubao, TtsPlaybackEvent::Started);
        });
        self.draw();
    }

    fn advance(&mut self, app: &Entity<MultisoulApp>, delta: Duration) {
        app.update(&mut self.cx, |app, cx| {
            app.voice_window_inset_view.update(cx, |view, cx| {
                view.drain_events();
                view.inset.tick(delta);
                view.publish_state();
                cx.notify();
            });
        });
        self.draw();
    }
}

/// 场景：工作空间项目的豆包播放必须启动光晕帧驱动，并在窗口层绘制。
/// 数据构造：隔离存储中的宿主和两个项目 Entity，项目均有 desktop_host；
///   窗口 1280×800，120ms 等于现有淡入时长，零网络、麦克风及真实 PTY。
/// 执行过程：注入当前项目 Started → 绘制并检查自然启动的帧驱动 →
///   推进淡入并核对窗口边界 → 打开设置 → 点击关闭设置。
/// 预期结果：光晕覆盖整个窗口且在设置打开时保留；不能被项目裁剪或挡住设置操作。
#[test]
fn desktop_doubao_inset_starts_driver_and_covers_window_below_settings() {
    let mut fixture = WorkspaceFixture::new();
    let project = fixture.projects[0].clone();
    fixture.start(&project);
    project.read_with(&fixture.cx, |app, cx| {
        assert!(
            app.voice_settings.values().voice_window_inset_enabled(),
            "光晕应默认开启"
        );
        assert!(
            app.voice_window_inset_view.read(cx).frame_pending,
            "当前项目的光晕视图必须进入真实渲染树并自然启动帧驱动"
        );
    });
    fixture.advance(&project, Duration::from_millis(120));
    assert!(
        fixture.cx.debug_bounds("desktop-project-world").is_some(),
        "项目应沿生产 Scene 路径绘制"
    );
    let glow = fixture
        .cx
        .debug_bounds("voice-window-inset")
        .expect("豆包回复时宿主必须绘制光晕");
    assert_eq!(glow.origin, point(px(0.0), px(0.0)), "光晕应从窗口原点开始");
    assert_eq!(
        glow.size,
        size(px(1280.0), px(800.0)),
        "光晕必须覆盖窗口及共享 Dock"
    );
    assert_ne!(
        glow.origin,
        point(px(14.0), px(64.0)),
        "光晕不能局限于项目 Scene"
    );
    project.update(&mut fixture.cx, |app, cx| {
        app.settings_panel.open = true;
        cx.notify();
    });
    fixture.host.update(&mut fixture.cx, |_, cx| cx.notify());
    fixture.draw();
    assert!(
        fixture
            .cx
            .debug_bounds("settings-reduced-motion-crossfade")
            .is_some(),
        "设置必须位于窗口层"
    );
    assert!(
        fixture.cx.debug_bounds("voice-window-inset").is_some(),
        "设置打开时仍应绘制光晕"
    );
    let close = fixture
        .cx
        .debug_bounds("settings-close")
        .expect("应保留设置关闭按钮");
    fixture.cx.simulate_click(close.center(), Modifiers::none());
    project.read_with(&fixture.cx, |app, _| {
        assert!(!app.settings_panel.open, "光晕不能拦截设置关闭按钮的点击");
    });
}

/// 场景：宿主只呈现当前项目的光晕，项目切换、概览和独立画布不能串用播放状态。
/// 数据构造：同一窗口的项目 A/B 具有独立 EventBus；120ms 为淡入时长。
/// 执行过程：A 播放并显光 → 切到未播放的 B → B 播放 → 返回概览 →
///   切到独立 Canvas 并播放宿主自身的语音。
/// 预期结果：A/B 各自激活时可见；未播放的 B 与概览不残留旧光晕；独立画布仍正常。
#[test]
fn desktop_doubao_inset_tracks_current_project_and_preserves_standalone_canvas() {
    let mut fixture = WorkspaceFixture::new();
    let [a, b] = fixture.projects.clone();
    fixture.start(&a);
    fixture.advance(&a, Duration::from_millis(120));
    assert!(
        fixture.cx.debug_bounds("voice-window-inset").is_some(),
        "当前项目 A 应显示光晕"
    );
    fixture.host.update(&mut fixture.cx, |app, cx| {
        app.desktop_workspace.expanded_area_id = Some(fixture.area_ids[1].clone());
        cx.notify();
    });
    fixture.draw();
    assert!(
        fixture.cx.debug_bounds("voice-window-inset").is_none(),
        "切换到未播音的 B 后不得残留 A 的光晕"
    );
    fixture.start(&b);
    fixture.advance(&b, Duration::from_millis(120));
    assert!(
        fixture.cx.debug_bounds("voice-window-inset").is_some(),
        "当前项目 B 的播放也必须显示光晕"
    );
    fixture.host.update(&mut fixture.cx, |app, cx| {
        app.desktop_workspace.expanded_area_id = None;
        cx.notify();
    });
    fixture.draw();
    assert!(
        fixture.cx.debug_bounds("voice-window-inset").is_none(),
        "概览不能绘制后台项目的光晕"
    );
    fixture.host.update(&mut fixture.cx, |app, cx| {
        app.screen = AppScreen::Canvas;
        cx.notify();
    });
    let host = fixture.host.clone();
    fixture.start(&host);
    fixture.advance(&host, Duration::from_millis(120));
    assert!(
        fixture.cx.debug_bounds("voice-window-inset").is_some(),
        "独立画布的既有光晕必须继续显示"
    );
}
