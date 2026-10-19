use crate::app::MultisoulApp;
use crate::preview::local_content::LocalPreview;
use gpui::TestAppContext;
use std::path::Path;
use workspace::content::Project;

/// 场景：切换工作空间时，旧正文请求和新正文请求使用相同递增序号。
/// 数据构造：A、B 两个项目都有 note.txt，分别含不同正文。
/// 执行过程：启动 A 读取后立刻替换工作台状态，并读取 B；再排空任务。
/// 预期结果：最终正文只能来自 B，旧实例回调不能覆盖新实例。
#[test]
fn late_content_from_previous_workbench_cannot_replace_current_document() {
    let temp = tempfile::tempdir().expect("目录");
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    std::fs::create_dir(&a).expect("A目录");
    std::fs::create_dir(&b).expect("B目录");
    std::fs::write(a.join("note.txt"), "from A").expect("A正文");
    std::fs::write(b.join("note.txt"), "from B").expect("B正文");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("存储");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    app.update(cx, |app, cx| {
        app.desktop_workspace.workbench.project = Some(Project::open(&a).expect("A项目"));
        app.desktop_workspace.workbench.saved.selected = Some("note.txt".into());
        app.load_workbench_file(false, false, cx);
        app.desktop_workspace.workbench = Default::default();
        app.desktop_workspace.workbench.project = Some(Project::open(&b).expect("B项目"));
        app.desktop_workspace.workbench.saved.selected = Some("note.txt".into());
        app.load_workbench_file(false, false, cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace
                .workbench
                .file
                .as_ref()
                .expect("正文")
                .text,
            "from B",
            "迟到A请求不能替换B正文"
        );
        assert!(
            app.desktop_workspace.workbench.error.is_none(),
            "B读取没有错误"
        );
    });
}

/// 场景：Markdown 编辑保存后立刻切回预览。
/// 数据构造：磁盘文档为旧标题，编辑器草稿替换为新标题。
/// 执行过程：经过真实保存路径，再请求更新后的本地预览 URL。
/// 预期结果：磁盘和富文本预览都显示新标题，草稿不再脏。
#[test]
fn markdown_save_refreshes_rendered_preview() {
    let temp = tempfile::tempdir().expect("目录");
    std::fs::write(temp.path().join("note.md"), "# OLD TITLE").expect("原文");
    let project = Project::open(temp.path()).expect("项目");
    let file = project.read_text(Path::new("note.md")).expect("读取");
    let preview = LocalPreview::new(temp.path(), "note.md", Some(&file.text)).expect("预览");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("存储");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    app.update_in(cx, |app, window, cx| {
        app.desktop_workspace.workbench.project = Some(project);
        app.desktop_workspace.workbench.saved.selected = Some("note.md".into());
        app.desktop_workspace.workbench.file = Some(file);
        app.desktop_workspace.workbench.preview = Some(preview);
        app.edit_workbench_file(window, cx);
        let key = app.desktop_workspace.workbench.draft_key().expect("草稿键");
        app.desktop_workspace.workbench.drafts[&key]
            .input
            .clone()
            .update(cx, |input, cx| input.set_value("# NEW TITLE", window, cx));
        app.desktop_workspace
            .workbench
            .drafts
            .get_mut(&key)
            .expect("草稿")
            .dirty = true;
        app.save_workbench_file(cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        let s = &app.desktop_workspace.workbench;
        let preview = s.preview.as_ref().expect("更新后预览");
        let response = crate::preview::local_content::respond(
            wry::http::Request::builder()
                .uri(&preview.url)
                .body(Vec::new())
                .expect("请求"),
        );
        let html = String::from_utf8_lossy(response.body());
        assert!(html.contains("NEW TITLE"), "预览必须显示新标题");
        assert!(!html.contains("OLD TITLE"), "预览不得保留旧标题");
        assert!(!s.has_edits(), "成功保存后无脏草稿");
    });
    assert_eq!(
        std::fs::read_to_string(temp.path().join("note.md")).expect("磁盘"),
        "# NEW TITLE",
        "保存实际落盘"
    );
}

/// 场景：项目选择器已切到 B，A 的后台刷新尚未回填。
/// 数据构造：A 有未跟踪 a.txt，UI 所选区域却是 B，旧 project/snapshot 仍来自 A。
/// 执行过程：请求暂存 a.txt，再排空后台。
/// 预期结果：拒绝旧仓库能力，A 的索引保持不变，不启动 Git 写任务。
#[test]
fn switched_project_cannot_apply_old_repository_action() {
    use workspace::content::{Comparison, GitAction};
    let temp = tempfile::tempdir().expect("目录");
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    std::fs::create_dir(&a).expect("A目录");
    std::fs::create_dir(&b).expect("B目录");
    let init = std::process::Command::new("/usr/bin/git")
        .args(["init", "-q"])
        .current_dir(&a)
        .status()
        .expect("Git");
    assert!(init.success(), "创建隔离仓库");
    std::fs::write(a.join("a.txt"), "a").expect("A文件");
    let project = Project::open(&a).expect("A项目");
    let snapshot = project.git_snapshot(&Comparison::Local).expect("A状态");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("switch").expect("空间");
    let area_a = catalog.add_project(&workspace.id, &a).expect("A区域");
    let area_b = catalog.add_project(&workspace.id, &b).expect("B区域");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    app.update(cx, |app, cx| {
        app.desktop_workspace.areas = vec![area_a, area_b.clone()];
        let s = &mut app.desktop_workspace.workbench;
        s.saved.area_id = Some(area_b.id);
        s.project = Some(project.clone());
        s.git = Some(snapshot);
        app.workbench_git_action(GitAction::Stage(vec!["a.txt".into()]), cx);
        assert!(
            !app.desktop_workspace.workbench.busy,
            "根目录不一致时不得启动写操作"
        );
    });
    cx.run_until_parked();
    assert!(
        !project
            .git_snapshot(&Comparison::Local)
            .expect("A状态")
            .changes[0]
            .staged,
        "不能误暂存旧项目"
    );
}

/// 场景：编辑后切到网页并收起右栏，再请求退出。
/// 数据构造：隔离空间持久化工作台与网页标签，文件草稿标为 dirty。
/// 执行过程：调用离开保护，再排空标签选择任务。
/// 预期结果：退出被拦截，右栏打开且文件工作台选中，提示具备可见载体。
#[test]
fn leave_guard_reveals_workbench_from_closed_browser_drawer() {
    let temp = tempfile::tempdir().expect("目录");
    std::fs::write(temp.path().join("note.txt"), "original").expect("原文");
    let project = Project::open(temp.path()).expect("项目");
    let file = project.read_text(Path::new("note.txt")).expect("正文");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("存储");
    let workspace = store
        .desktop_workspace_store()
        .create("leave")
        .expect("空间");
    let tabs = storage::DesktopContentTabStore::new(paths.app_db_path().to_owned());
    for (kind, resource, title) in [
        (
            storage::DesktopContentKind::ProjectFiles,
            "project-files",
            "文件",
        ),
        (storage::DesktopContentKind::Browser, "about:blank", "网页"),
    ] {
        tabs.open_or_select(storage::NewDesktopContentTab {
            workspace_id: workspace.id.clone(),
            source_area_id: None,
            source_project_name: "leave".into(),
            kind,
            resource: resource.into(),
            version: None,
            title: title.into(),
        })
        .expect("标签");
    }
    let loaded = tabs.list(&workspace.id).expect("读取标签");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    app.update_in(cx, |app, window, cx| {
        app.desktop_workspace.active_id = Some(workspace.id);
        app.desktop_workspace.content_tabs = loaded;
        let s = &mut app.desktop_workspace.workbench;
        s.project = Some(project);
        s.saved.selected = Some("note.txt".into());
        s.file = Some(file);
        app.edit_workbench_file(window, cx);
        let key = app.desktop_workspace.workbench.draft_key().expect("键");
        app.desktop_workspace
            .workbench
            .drafts
            .get_mut(&key)
            .expect("草稿")
            .dirty = true;
        app.desktop_workspace.drawers.right_open = false;
        assert!(
            !app.confirm_preview_edits_before_leaving(cx),
            "不能丢弃修改退出"
        );
        assert!(
            app.desktop_workspace.drawers.right_open,
            "退出决策提示必须可见"
        );
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace
                .content_tabs
                .iter()
                .any(|t| t.selected && t.kind == storage::DesktopContentKind::ProjectFiles),
            "重新选中工作台以显示离开提示"
        );
        assert!(
            app.desktop_workspace.workbench.pending_close.is_some(),
            "保留退出意图"
        );
    });
}

/// 场景：助手直接打开 Markdown 文件，不能退回旧的纯文本渲染路径。
/// 数据构造：工作空间中的独立 File 标签指向带标题的临时文档。
/// 执行过程：按标签的真实按需加载入口加载，再请求预览页面。
/// 预期结果：正文和 HTML 预览同时就绪，后者含 h1 而不是 Markdown 原始标记。
#[test]
fn assistant_file_tab_uses_shared_rich_preview() {
    let temp = tempfile::tempdir().expect("目录");
    let path = temp.path().join("note.md");
    std::fs::write(&path, "# Assistant document").expect("文档");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("存储");
    let workspace = store
        .desktop_workspace_store()
        .create("assistant preview")
        .expect("空间");
    let tab_store = storage::DesktopContentTabStore::new(paths.app_db_path().to_owned());
    let tab = tab_store
        .open_or_select(storage::NewDesktopContentTab {
            workspace_id: workspace.id.clone(),
            source_area_id: None,
            source_project_name: "assistant preview".into(),
            kind: storage::DesktopContentKind::File,
            resource: path.to_string_lossy().to_string(),
            version: None,
            title: "note.md".into(),
        })
        .expect("标签");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    app.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some(workspace.id);
        app.desktop_workspace.content_tabs = vec![tab.clone()];
        app.load_selected_desktop_content(cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        let preview = app
            .desktop_workspace
            .content_resources
            .get(&tab.id)
            .expect("富文本预览应就绪");
        let response = crate::preview::local_content::respond(
            wry::http::Request::builder()
                .uri(&preview.url)
                .body(Vec::new())
                .expect("请求"),
        );
        assert!(
            String::from_utf8_lossy(response.body()).contains("<h1>Assistant document</h1>"),
            "助手入口必须渲染 Markdown"
        );
        assert_eq!(
            app.desktop_workspace.content_previews[&tab.id]
                .as_ref()
                .expect("原文"),
            "# Assistant document",
            "保留原文供源码视图使用"
        );
    });
}
