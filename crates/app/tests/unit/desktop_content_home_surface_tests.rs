#[path = "../support/drawer_locale.rs"]
mod drawer_locale;
use drawer_locale::expected_copy;

use super::home_tools::HomeTool;
use crate::app::MultisoulApp;
use crate::shortcuts::{default_shortcut_entries, ShortcutCommandId};
use gpui::{px, size, AppContext, Entity, TestAppContext, VisualTestContext};
use runtime::desktop_drawers::DesktopDrawerEvent;

fn open_right_drawer<'a>(
    test: &'a mut TestAppContext,
    root: &std::path::Path,
    name: &str,
) -> (Entity<MultisoulApp>, &'a mut VisualTestContext, String) {
    let paths = storage::DatabasePaths::isolated_test(&root.join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = store
        .desktop_workspace_store()
        .create(name)
        .expect("应创建工作空间");
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
        app.finish_desktop_zoom(window, cx);
        app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleRight, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    (host, cx, workspace.id)
}

/// 右栏空态应按原型呈现胶囊地址栏、两列工具磁贴与推荐宫格。
///
/// 数据构造：1280×800 空工作空间，只展开右栏，没有任何内容标签与任务成果。
/// 执行过程：绘制真实 GPUI 树，读取地址栏、六个工具磁贴与四个推荐位的边界。
/// 预期结果：地址栏高 36px；工具按“变更/终端、文件/项目进度、网页/更多工具”两列三行排列，
/// 每格高 40px；推荐区有 4 个位置；不再出现独立的“打开网页”按钮。
#[test]
fn empty_reader_matches_prototype_new_tab_layout() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let mut test = TestAppContext::single();
    let (_host, cx, _) = open_right_drawer(&mut test, temp.path(), "Home Layout");
    let address = cx
        .debug_bounds("desktop-home-address")
        .expect("空态顶部应有胶囊地址栏");
    assert_eq!(address.size.height, px(36.0), "地址栏高度应为原型的 36px");
    assert!(
        cx.debug_bounds("desktop-home-open-url").is_none(),
        "回车即可打开网页，不应再有独立的“打开网页”按钮"
    );
    let mut tile = |tool: HomeTool| {
        cx.debug_bounds(tool.selector())
            .unwrap_or_else(|| panic!("应显示工具磁贴 {}", tool.selector()))
    };
    let rows = [
        (HomeTool::Changes, HomeTool::Terminal),
        (HomeTool::Files, HomeTool::Progress),
        (HomeTool::Web, HomeTool::More),
    ];
    let mut previous_top = None;
    for (left, right) in rows {
        let (a, b) = (tile(left), tile(right));
        assert_eq!(a.size.height, px(40.0), "工具磁贴高度应为 40px");
        assert_eq!(a.top(), b.top(), "同一行的两个磁贴应顶部对齐");
        assert!(a.right() <= b.left(), "左列磁贴应位于右列左侧");
        if let Some(top) = previous_top {
            assert!(a.top() > top, "下一行应排在上一行之下");
        }
        previous_top = Some(a.top());
    }
    let more = tile(HomeTool::More);
    let slots = [
        "desktop-home-rec-0",
        "desktop-home-rec-1",
        "desktop-home-rec-2",
        "desktop-home-rec-3",
    ];
    for (index, selector) in slots.into_iter().enumerate() {
        let rec = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("推荐区应有第 {index} 个位置"));
        assert!(rec.top() > more.bottom(), "推荐区应位于工具网格之下");
    }
}

/// 尚未实现的工具保持原型外观，点击时就地提示“即将支持”，不打开任何标签。
///
/// 数据构造：同上的空右栏。
/// 执行过程：依次激活“项目进度”和“更多工具”磁贴，重绘。
/// 预期结果：出现 desktop-home-notice 提示且文案为“即将支持”；内容标签数量仍为 0。
#[test]
fn upcoming_home_tools_show_inline_notice_without_opening_tabs() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let mut test = TestAppContext::single();
    let (host, cx, _) = open_right_drawer(&mut test, temp.path(), "Home Upcoming");
    for tool in [HomeTool::Progress, HomeTool::More] {
        host.update_in(cx, |app, window, cx| {
            app.activate_content_home_tool(tool, window, cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        assert!(
            cx.debug_bounds("desktop-home-notice").is_some(),
            "{} 应显示就地提示",
            tool.selector()
        );
        host.update(cx, |app, _| {
            assert!(
                app.desktop_workspace.content_tabs.is_empty(),
                "展望入口不应打开标签"
            );
            assert_eq!(
                app.desktop_workspace.content_browser.home_notice.as_deref(),
                Some(expected_copy("即将支持", "Coming soon")),
                "提示文案应为“即将支持”"
            );
        });
    }
}

/// 网页标签下不再在正文顶部叠“文件 / +”工具行，新建网页统一放在标签条末尾。
///
/// 数据构造：右栏放入一个已选中的 https://example.com 网页标签。
/// 执行过程：重绘并读取标签条与浏览器导航的边界。
/// 预期结果：标签条末尾有 desktop-content-new-browser；正文导航中没有 desktop-content-tool-row。
#[test]
fn browser_tab_drops_tool_row_and_keeps_new_tab_button_in_strip() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let mut test = TestAppContext::single();
    let (host, cx, workspace_id) = open_right_drawer(&mut test, temp.path(), "Home Strip");
    host.update_in(cx, |app, _, cx| {
        app.browsers.disable_native_views();
        app.desktop_workspace.content_tabs = vec![storage::DesktopContentTab {
            instance_id: String::new(),
            preview_type: None,
            id: "example".into(),
            workspace_id: workspace_id.clone(),
            source_area_id: None,
            source_project_name: "Home Strip".into(),
            kind: storage::DesktopContentKind::Browser,
            resource: "https://example.com/".into(),
            version: None,
            title: "Example".into(),
            position: 0,
            selected: true,
            source_removed: false,
        }];
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let chrome = cx
        .debug_bounds("desktop-content-chrome")
        .expect("应找到标签条");
    let plus = cx
        .debug_bounds("desktop-content-new-browser")
        .expect("网页标签下标签条末尾也应有新建网页按钮");
    assert!(
        plus.top() >= chrome.top() && plus.bottom() <= chrome.bottom(),
        "新建网页按钮应位于标签条内"
    );
    assert!(
        cx.debug_bounds("desktop-content-tool-row").is_none(),
        "网页正文不应再叠“文件 / +”工具行"
    );
}

/// 文件与变更磁贴需要可在设置中修改的默认快捷键，分别为 ⌘P 与 ⌃⇧G。
///
/// 数据构造：读取默认快捷键目录。
/// 执行过程：查找 OpenFileWorkbench 与 ShowChanges 两个命令。
/// 预期结果：默认键分别为 cmd-p 与 ctrl-shift-g，且可编辑。
#[test]
fn files_and_changes_tools_have_editable_default_shortcuts() {
    let entries = default_shortcut_entries();
    for (command, keystroke) in [
        (ShortcutCommandId::OpenFileWorkbench, "cmd-p"),
        (ShortcutCommandId::ShowChanges, "ctrl-shift-g"),
    ] {
        let entry = entries
            .iter()
            .find(|entry| entry.command == command)
            .unwrap_or_else(|| panic!("快捷键目录应包含 {command:?}"));
        assert_eq!(entry.default_keystroke, Some(keystroke));
        assert!(entry.editable, "{command:?} 应可在设置中录制修改");
    }
}
