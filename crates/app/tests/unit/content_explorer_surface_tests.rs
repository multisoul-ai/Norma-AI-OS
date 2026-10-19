use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, size, Context, Entity, Render, TestAppContext, Window};

struct WorkbenchHost {
    app: Entity<MultisoulApp>,
    width: f32,
}

impl Render for WorkbenchHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = storage::DesktopContentTab {
            id: "files".into(),
            kind: storage::DesktopContentKind::ProjectFiles,
            resource: "project-files".into(),
            title: "文件".into(),
            workspace_id: "w".into(),
            source_area_id: None,
            source_project_name: "p".into(),
            version: None,
            position: 0,
            selected: true,
            source_removed: false,
        };
        let width = self.width;
        let body = self.app.update(cx, |app, cx| {
            app.render_file_workbench(&tab, width, window, cx)
        });
        div().w(px(width)).h(px(600.0)).child(body)
    }
}

fn draw(width: f32, selected: bool) -> (TestAppContext, Option<(f32, f32)>, bool, bool) {
    let temp = tempfile::tempdir().expect("目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("存储");
    let mut test = TestAppContext::single();
    let (host, cx) = test.add_window_view(|_, cx| WorkbenchHost {
        app: cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)),
        width,
    });
    cx.simulate_resize(size(px(width), px(600.0)));
    if selected {
        host.update(cx, |host, cx| {
            host.app.update(cx, |app, _| {
                app.desktop_workspace.workbench.saved.selected = Some("note.txt".into());
            })
        });
    }
    cx.update(|window, cx| window.draw(cx).clear());
    let doc = cx
        .debug_bounds("files-path-bar")
        .map(|b| b.size.width.into());
    let tree = cx
        .debug_bounds("files-sidebar")
        .map(|b| b.size.width.into());
    let toggle = cx.debug_bounds("files-toggle-tree").is_some();
    let context_bar = cx.debug_bounds("desktop-content-context").is_some();
    let widths = doc.zip(tree);
    drop(temp);
    (test, widths, toggle, context_bar)
}

/// 场景：565px 与 474px 常规右栏必须正文与目录并排，且没有通用上下文条。
/// 数据构造：直接渲染文件工作台，已选中 note.txt，宽度 565 / 474。
/// 执行过程：绘制一帧并读取路径条与目录的真实包围盒。
/// 预期结果：565→正文 383.2 + 目录 181.8（含 1px 分隔线）；474→正文 303 + 目录 171；
/// 两种宽度都没有紧凑目录按钮，也没有 desktop-content-context。
#[test]
fn workbench_shows_document_and_tree_side_by_side_without_context_bar() {
    for (width, doc, tree) in [(565.0, 383.2, 181.8), (474.0, 303.0, 171.0)] {
        let (_t, widths, toggle, context) = draw(width, true);
        let (d, t) = widths.expect("正文与目录都应绘制");
        assert!((d - doc).abs() < 0.6, "{width}px 正文宽 {d}，期望 {doc}");
        assert!((t - tree).abs() < 0.6, "{width}px 目录宽 {t}，期望 {tree}");
        assert!(!toggle, "{width}px 不应出现紧凑目录按钮");
        assert!(!context, "{width}px 文件页不得绘制通用上下文条");
    }
}

/// 场景：185px 极窄栏不并排，但文件入口不丢失。
/// 数据构造：宽度 185，已选中文件。
/// 执行过程：绘制一帧。
/// 预期结果：正文占满 185，没有目录；路径条里有「显示文件目录」按钮可进入目录。
#[test]
fn narrow_workbench_keeps_a_discoverable_tree_button() {
    let (_t, widths, toggle, _) = draw(185.0, true);
    assert!(widths.is_none(), "紧凑模式下目录默认不并排: {widths:?}");
    assert!(toggle, "紧凑模式必须保留目录入口按钮");
}
