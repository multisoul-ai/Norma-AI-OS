use super::project_picker::ProjectPicker;
use std::path::PathBuf;
use workspace::{WorkspaceRecord, WorkspaceSource};

struct PickerTestView;

impl gpui::Render for PickerTestView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::prelude::*;
        gpui::div()
            .size_full()
            .children(ui::component::Root::render_dialog_layer(window, cx))
    }
}

/// 添加项目可搜索全部已保存历史，同名项目按路径区分。
///
/// 数据构造：50条记录=存储的历史容量；前48条为普通路径，末两条同名“官网”，
///   路径分别为/team-a/官网与/team-b/官网。
/// 执行过程：默认列表 → 按中文名称搜索 → 按大写路径搜索 → 无结果 → 清空。
/// 预期结果：不截断为6条；同名显示两条；路径搜索只命中team-b，无结果不能确认旧项目。
#[test]
fn desktop_project_picker_searches_all_saved_projects() {
    let mut records: Vec<_> = (0..48)
        .map(|i| {
            WorkspaceRecord::new(
                PathBuf::from(format!("/projects/project-{i}")),
                0,
                WorkspaceSource::Open,
            )
        })
        .collect();
    let a = PathBuf::from("/team-a/官网");
    let b = PathBuf::from("/team-b/官网");
    records.push(WorkspaceRecord::new(a.clone(), 0, WorkspaceSource::Open));
    records.push(WorkspaceRecord::new(b.clone(), 0, WorkspaceSource::Open));
    let mut picker = ProjectPicker::new(records, ui::Theme::sapphire_night());
    assert_eq!(picker.matching.len(), 50, "添加项目不能只显示前6条历史");
    picker.search(" 官网 ");
    assert_eq!(picker.matching.len(), 2, "同名项目应保留不同路径");
    assert_eq!(
        picker.project_at(0).unwrap().path,
        a,
        "第一项路径应保持原顺序"
    );
    assert_eq!(
        picker.project_at(1).unwrap().path,
        b,
        "不能因同名去重另一目录"
    );
    picker.search("TEAM-B");
    assert_eq!(picker.matching.len(), 1, "路径搜索应忽略大小写");
    assert_eq!(
        picker.project_at(0).unwrap().path,
        b,
        "第50条仍然必须可搜索"
    );
    picker.search("missing-project");
    assert!(picker.project_at(0).is_none(), "无结果后不能继续确认旧项目");
    picker.search("");
    assert_eq!(picker.matching.len(), 50, "清空搜索应恢复全部历史");
}

/// 列表组件在小窗口保持有界，键盘与鼠标可重复添加同项目，取消不添加记录。
///
/// 数据构造：50个存在的目录（与历史容量一致），一个空工作空间，窗口800×600。
///   前两条名称分别为1字符和32字符，验证长度不改变左对齐；行高上限52px=两行正文36px+上下留白16px。
///   项目49位于第50条；组件搜索防闪等待100ms，再留20ms调度余量。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开列表 → 绘制首屏并检查名称左对齐、行高和取消按钮位置。
///   2. 搜索末条并按 Enter → 添加 project-49，关闭选择器。
///   3. 重开并点击已添加的 project-49 → 创建第二张独立画布，关闭选择器。
///   4. 重开并按 Escape → 只关闭选择器，不添加画布。
///   5. 重开搜索 project-48 并单击 → 添加另一项目，验证鼠标确认仍有效。
///
/// 预期结果：
///   - 正断言：名称左对齐，底栏在 600px 内；project-49 有两张独立画布，project-48 有一张。
///   - 负断言：末条不在首屏绘制，不误加 project-0；Escape 不增加画布。
#[test]
fn desktop_project_picker_routes_keyboard_and_bounds_long_lists() {
    use crate::app::{AppScreen, MultisoulApp};
    use gpui::{px, size, AppContext, TestAppContext};
    use ui::component::Root;

    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Picker review").expect("创建空空间");
    let mut records: Vec<_> = (0..50)
        .map(|i| {
            let path = temp.path().join(format!("project-{i}"));
            std::fs::create_dir(&path).expect("创建项目目录");
            WorkspaceRecord::new(
                path.canonicalize().expect("规范项目路径"),
                0,
                WorkspaceSource::Open,
            )
        })
        .collect();
    records[0].display_name = "A".into();
    records[1].display_name = "A project with a much longer name".into();
    storage::RecentStore::new(paths.app_db_path().to_owned())
        .save(&workspace::RecentWorkspaces {
            records: records.clone(),
        })
        .expect("保存50条历史，添加项目刷新后仍可继续选择");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        // Native AppKit focus belongs to real App acceptance; this harness renders only the picker.
        Root::new(cx.new(|_| PickerTestView), window, cx)
    });
    let host = host.expect("宿主存在");
    cx.simulate_resize(size(px(800.0), px(600.0)));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.workspace_state.recent.records = records;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.screen = AppScreen::DesktopWorkspace;
        app.open_desktop_add_project(window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let footer = cx
        .debug_bounds("desktop-add-project-cancel")
        .expect("底栏存在");
    assert!(
        footer.bottom() < px(600.0),
        "50条项目不能将取消按钮挤出窗口"
    );
    assert!(
        cx.debug_bounds("desktop-add-recent-0").is_some(),
        "首条应可见"
    );
    assert!(
        cx.debug_bounds("desktop-add-recent-49").is_none(),
        "末条应由列表虚拟化，不在首屏绘制"
    );
    let row = cx.debug_bounds("desktop-add-recent-0").expect("首行存在");
    let short = cx.debug_bounds("desktop-add-name-0").expect("短名称存在");
    let long = cx.debug_bounds("desktop-add-name-1").expect("长名称存在");
    assert_eq!(
        short.left(),
        long.left(),
        "不同长度的项目名称必须左对齐，不能各自居中"
    );
    // 名称左侧最多64px = 左留白12px + 图标容器32px + 间距12px + 容差8px。
    assert!(
        short.left() - row.left() <= px(64.0),
        "项目名称不能漂浮在行中央"
    );
    assert!(row.size.height <= px(52.0), "紧凑列表应让更多项目同时可见");
    cx.simulate_input("project-49");
    cx.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(120));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-add-recent-49").is_some(),
        "搜索后第50条应可见"
    );
    assert!(
        cx.debug_bounds("desktop-add-recent-0").is_none(),
        "搜索不能保留不匹配行"
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let areas = catalog
        .list_projects(&workspace.id)
        .expect("读取已加入项目");
    assert_eq!(areas.len(), 1, "Enter必须只加入一个项目");
    assert_eq!(
        areas[0].root,
        temp.path().join("project-49").canonicalize().unwrap(),
        "必须确认末尾项目，而不是旧选择"
    );
    assert_ne!(
        areas[0].root,
        temp.path().join("project-0").canonicalize().unwrap(),
        "不能误加第一条"
    );
    host.update_in(cx, |app, window, cx| {
        app.open_desktop_add_project(window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_input("project-49");
    cx.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(120));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let added_row = cx
        .debug_bounds("desktop-add-recent-0")
        .expect("已添加行存在");
    cx.simulate_click(added_row.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let repeated = catalog
        .list_projects(&workspace.id)
        .expect("读取重复添加结果");
    assert_eq!(repeated.len(), 2, "已添加项目必须仍能重复加入");
    assert_ne!(repeated[0].id, repeated[1].id, "重复添加必须有两个独立画布");
    assert_eq!(
        repeated[0].root, repeated[1].root,
        "重复添加应保持项目目录一致"
    );
    assert!(
        !cx.update(|_, cx| host.read(cx).desktop_workspace.add_project_open),
        "重复添加成功后应关闭选择器"
    );
    host.update_in(cx, |app, window, cx| {
        app.open_desktop_add_project(window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        !cx.update(|_, cx| host.read(cx).desktop_workspace.add_project_open),
        "Escape必须关闭对话框并释放打开状态"
    );
    assert_eq!(
        catalog
            .list_projects(&workspace.id)
            .expect("读取取消后目录")
            .len(),
        2,
        "取消不得新增第三张画布"
    );
    let mouse_index = cx.update(|_, cx| {
        host.read(cx)
            .workspace_state
            .recent
            .records
            .iter()
            .position(|record| record.path.ends_with("project-48"))
            .expect("project-48仍在历史中")
    });
    host.update_in(cx, |app, window, cx| {
        app.open_desktop_add_project(window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_input("project-48");
    cx.run_until_parked();
    // 与上述搜索相同：组件防闪100ms + 调度余量20ms = 120ms。
    std::thread::sleep(std::time::Duration::from_millis(120));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let available = cx
        .debug_bounds(format!("desktop-add-recent-{mouse_index}").leak())
        .expect("可添加行存在");
    cx.simulate_click(available.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let areas = catalog.list_projects(&workspace.id).expect("读取添加结果");
    assert_eq!(
        areas.len(),
        3,
        "鼠标确认必须在两张project-49之外新增project-48一次"
    );
    assert!(
        areas.iter().any(|area| area.root.ends_with("project-48")),
        "鼠标选中的project-48应已加入"
    );
    assert!(
        areas.iter().any(|area| area.root.ends_with("project-49")),
        "键盘添加的project-49应保留"
    );
    assert!(
        !areas.iter().any(|area| area.root.ends_with("project-0")),
        "不能误加未选中的project-0"
    );
}
