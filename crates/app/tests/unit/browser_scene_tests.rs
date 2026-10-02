use super::*;
use crate::app::{AppScreen, MultisoulApp};
use gpui::{
    div, point, prelude::*, px, scene, size, AppContext, Context, Entity, IntoElement, Render,
    TestAppContext, Window,
};

struct BrowserSceneHost {
    app: Entity<MultisoulApp>,
    shifted: bool,
    visible: bool,
}

impl Render for BrowserSceneHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().when(self.visible, |view| {
            let browser = self.app.update(cx, |app, cx| {
                app.browser_scene_element(
                    7,
                    "https://example.com/".into(),
                    BrowserViewBounds {
                        x: 40.,
                        y: 20.,
                        width: 80.,
                        height: 60.,
                        bottom_corner_radius_px: 8.,
                    },
                    cx,
                )
            });
            view.child(
                scene(
                    "browser-camera",
                    size(px(200.), px(100.)),
                    div().relative().size_full().child(browser),
                )
                .scale(if self.shifted { 1. } else { 0.5 })
                .offset(if self.shifted {
                    point(px(150.), px(50.))
                } else {
                    point(px(20.), px(10.))
                }),
            )
        })
    }
}

/// Browser生产呈现入口保留网页逻辑尺寸、跨项目展示实例以及隐藏状态。
///
/// 数据构造：窗口200×120，Browser逻辑框(40,20,80,60)，节点ID7；
///   首帧镜头1/2+(20,10) → (40,20,40,30)；末帧1+(150,50) → (190,70,80,60)。
///   原生创建在headless测试中禁用，只验证真实GPUI帧→BrowserStore分发与生命周期。
/// 执行过程：绘制生产Browser元素 → 展开当前项目 → 切到另一项目 → 变更镜头 → 将项目detach为总览会话
///   → 切到目录禁止原生输入 → 移出渲染树。
/// 预期结果：逻辑尺寸一直80×60，呈现坐标跟随镜头；detach不删除Browser条目；
///   仅展开的当前项目允许输入；总览、其他项目和目录不允许输入；离场不能残留可点击原生层。
#[test]
fn browser_scene_preserves_document_and_detached_session() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let mut test = TestAppContext::single();
    let host =
        test.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
    let app =
        test.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
    let other = test.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
    host.update(&mut test, |host, _| {
        host.screen = AppScreen::DesktopWorkspace
    });
    app.update(&mut test, |app, _| {
        app.browsers.disable_native_views();
        app.desktop_host = Some(host.downgrade());
    });
    let (root, cx) = test.add_window_view(|_, _| BrowserSceneHost {
        app: app.clone(),
        shifted: false,
        visible: true,
    });
    cx.simulate_resize(size(px(200.), px(120.)));
    cx.update(|window, cx| window.draw(cx).clear());
    app.read_with(cx, |app, _| {
        let view = &app.browsers.views[&7];
        let (placement, interactive, _) = view.presentation.expect("必须从paint同步Browser呈现");
        assert_eq!(
            placement.bounds.origin,
            point(px(40.), px(20.)),
            "原生坐标必须经过镜头投影"
        );
        assert_eq!(
            placement.bounds.size,
            size(px(40.), px(30.)),
            "缩略画布必须整体缩小网页"
        );
        assert!(!interactive, "项目总览网页不得截获用于展开项目的点击");
    });
    host.update(cx, |host, _| {
        host.desktop_workspace.active_id = Some("workspace".into());
        host.desktop_workspace.expanded_area_id = Some("browser-project".into());
        host.desktop_workspace
            .sessions
            .insert(("workspace".into(), "browser-project".into()), app.clone());
        host.desktop_workspace
            .sessions
            .insert(("workspace".into(), "other-project".into()), other.clone());
    });
    cx.update(|window, cx| window.draw(cx).clear());
    app.read_with(cx, |app, _| {
        assert!(
            app.browsers.views[&7].presentation.unwrap().1,
            "展开的当前项目必须允许网页输入"
        );
    });
    host.update(cx, |host, _| {
        host.desktop_workspace.expanded_area_id = Some("other-project".into());
    });
    cx.update(|window, cx| window.draw(cx).clear());
    app.read_with(cx, |app, _| {
        assert!(
            !app.browsers.views[&7].presentation.unwrap().1,
            "展开其他项目不得解锁当前网页缩略图"
        );
    });
    root.update(cx, |root, cx| {
        root.shifted = true;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    app.update(cx, |app, cx| {
        let view = &app.browsers.views[&7];
        let (placement, _, _) = view.presentation.expect("移动后必须仍有原生呈现");
        assert_eq!(
            (view.bounds.width, view.bounds.height),
            (80., 60.),
            "镜头不得触发网页viewport resize"
        );
        assert_eq!(
            placement.visible_bounds.size,
            size(px(10.), px(50.)),
            "原生视图必须按窗口裁剪"
        );
        app.detach_project_window(cx);
        assert!(
            app.browsers.views.contains_key(&7),
            "离开展开态不能销毁网页会话"
        );
        assert_eq!(app.browsers.views.len(), 1, "切换不能复制网页条目");
    });
    host.update(cx, |host, _| host.screen = AppScreen::Launchpad);
    cx.update(|window, cx| window.draw(cx).clear());
    app.read_with(cx, |app, _| {
        assert!(
            !app.browsers.views[&7].presentation.unwrap().1,
            "目录二级缩略不得截获网页输入"
        );
    });
    root.update(cx, |root, cx| {
        root.visible = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    app.read_with(cx, |app, _| {
        let view = &app.browsers.views[&7];
        assert!(view.presentation.is_none(), "离场必须清理呈现状态");
        assert!(!view.visible, "离场不能残留可见原生层");
        assert_eq!(view.url, "https://example.com/", "离场不能清空原网页地址");
    });
}
