use super::project_picker::ProjectPicker;
use std::path::PathBuf;
use workspace::{WorkspaceRecord, WorkspaceSource};

/// 添加项目可搜索全部已保存历史，同名项目按路径区分，已添加状态不会混淆。
///
/// 数据构造：50条记录=存储的历史容量；前48条为普通路径，末两条同名“官网”，
///   路径分别为/team-a/官网与/team-b/官网；只有team-a已加入当前空间。
/// 执行过程：默认列表 → 按中文名称搜索 → 按大写路径搜索 → 无结果 → 清空。
/// 预期结果：不截断为6条；同名显示两条；路径搜索只命中team-b；已添加只标记team-a。
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
    let mut picker = ProjectPicker::new(records, [a.clone()].into(), ui::Theme::sapphire_night());
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
    assert!(picker.is_added(&a), "当前空间已有team-a，必须显示已添加");
    assert!(!picker.is_added(&b), "同名team-b不能被误判已添加");
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

/// 真实列表组件在小窗口保持有界，键盘可添加末尾项目，取消与已添加确认不产生重复记录。
///
/// 数据构造：50个存在的目录（与历史容量一致），一个空工作空间，窗口800×600。
///   项目49位于第50条；组件搜索防闪等待100ms，再留20ms调度余量。
/// 执行过程：打开 → 检查列表与底栏 → 搜索末条并Enter → 重开并重复确认 → Escape。
/// 预期结果：底栏在600px以内；只添加project-49且没有project-0；重复确认不关闭不添加；Escape关闭。
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
    let records: Vec<_> = (0..50)
        .map(|i| {
            let path = temp.path().join(format!("project-{i}"));
            std::fs::create_dir(&path).expect("创建项目目录");
            WorkspaceRecord::new(path, 0, WorkspaceSource::Open)
        })
        .collect();
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        Root::new(app, window, cx)
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
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        catalog.list_projects(&workspace.id).unwrap().len(),
        1,
        "已添加项目不能重复加入"
    );
    assert!(
        cx.update(|_, cx| host.read(cx).desktop_workspace.add_project_open),
        "确认禁用项不能关闭选择器"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        !cx.update(|_, cx| host.read(cx).desktop_workspace.add_project_open),
        "Escape必须关闭对话框并释放打开状态"
    );
}
