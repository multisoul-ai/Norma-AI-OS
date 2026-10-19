use canvas::{
    BrowserSpec, BrowserSurfaceKind, CanvasNodeKind, CanvasRect, Document, EgoLiteSpec, PreviewSpec,
};
use storage::{CanvasStore, DatabasePaths, DesktopWorkspaceStore, ProjectStore};

/// 清理旧桌面画布内容卡片时，只移除普通 Browser 与 Preview，保留 Ego 任务节点。
///
/// 数据构造：同一快照含普通 Browser、Preview、EgoLite Browser 和 Terminal 各一张。
/// 执行过程：保存后执行迁移两次，再重新读取快照。
/// 预期结果：首次删除前两张，第二次不再改动；Ego 和 Terminal 的 ID、内容保持。
#[test]
fn legacy_desktop_content_cleanup_is_idempotent_and_preserves_ego() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let store = CanvasStore::new(temp.path().join("canvas.sqlite"));
    let mut document = Document::new();
    let bounds = CanvasRect::new(0.0, 0.0, 400.0, 300.0);
    let browser = document.add_node(
        CanvasNodeKind::Browser(BrowserSpec {
            url: "https://example.test".to_string(),
            title: Some("旧网页".to_string()),
            surface: BrowserSurfaceKind::Native,
        }),
        bounds,
    );
    let preview = document.add_node(
        CanvasNodeKind::Preview(PreviewSpec {
            path: temp.path().join("plan.md").to_string_lossy().into_owned(),
            additional_paths: Vec::new(),
            explicit_type: None,
        }),
        bounds,
    );
    let ego = document.add_node(
        CanvasNodeKind::Browser(BrowserSpec {
            url: "https://task.example.test".to_string(),
            title: None,
            surface: BrowserSurfaceKind::EgoLite(EgoLiteSpec::default()),
        }),
        bounds,
    );
    let terminal = document.add_node(CanvasNodeKind::Term(Default::default()), bounds);
    store
        .save_snapshot(&document.to_snapshot())
        .expect("应保存旧画布");

    let removed = store
        .prune_desktop_content_cards()
        .expect("应清理普通内容卡片");
    let again = store.prune_desktop_content_cards().expect("重复清理应成功");
    let reloaded = store
        .load_snapshot()
        .expect("应读取清理后的画布")
        .expect("画布快照应保留");
    assert_eq!(removed, vec![browser, preview], "首次只应删除普通内容卡片");
    assert!(again.is_empty(), "重复清理不得再次修改快照");
    assert_eq!(reloaded.nodes.len(), 2, "清理后应保留两张运行卡片");
    assert_eq!(reloaded.nodes[0].id, ego, "Ego 节点身份必须保留");
    assert_eq!(reloaded.nodes[1].id, terminal, "Terminal 节点身份必须保留");
    assert!(
        matches!(
            &reloaded.nodes[0].kind,
            CanvasNodeKind::Browser(BrowserSpec {
                surface: BrowserSurfaceKind::EgoLite(_),
                ..
            })
        ),
        "保留的 Browser 必须是 EgoLite"
    );
    assert!(
        !reloaded.nodes.iter().any(|node| matches!(
            &node.kind,
            CanvasNodeKind::Preview(_)
                | CanvasNodeKind::Browser(BrowserSpec {
                    surface: BrowserSurfaceKind::Native,
                    ..
                })
        )),
        "旧普通 Browser 和 Preview 不得残留"
    );
}

/// 首页预览读取旧画布时也必须完成清理，避免未加载项目仍露出内容卡片。
///
/// 数据构造：一个尚未加载 Runtime 的桌面项目，快照含普通 Browser 和 Terminal。
/// 执行过程：读取工作空间首页预览，再读取原画布快照。
/// 预期结果：预览和磁盘均只保留 Terminal；普通 Browser 不被迁移为右侧标签。
#[test]
fn desktop_preview_catalog_prunes_legacy_content_before_display() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let catalog = DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("空间").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应创建项目区域");
    let project = ProjectStore::initialize_for_desktop_area(&paths, &root, &workspace.id, &area.id)
        .expect("应初始化画布存储");
    let mut document = Document::new();
    let bounds = CanvasRect::new(0.0, 0.0, 400.0, 300.0);
    let browser = document.add_node(
        CanvasNodeKind::Browser(BrowserSpec {
            url: "https://example.test".to_string(),
            title: None,
            surface: BrowserSurfaceKind::Native,
        }),
        bounds,
    );
    let terminal = document.add_node(CanvasNodeKind::Term(Default::default()), bounds);
    project
        .canvas_store()
        .save_snapshot(&document.to_snapshot())
        .expect("应保存旧画布");

    let previews = catalog.list_previews().expect("应加载首页预览");
    let nodes = &previews[&workspace.id][0].nodes;
    let saved = project
        .canvas_store()
        .load_snapshot()
        .expect("应重新读取画布")
        .expect("画布应保留");
    assert_eq!(nodes.len(), 1, "首页预览只应有 Terminal");
    assert_eq!(nodes[0].id, terminal, "首页应保留真实 Terminal 身份");
    assert_eq!(saved.nodes.len(), 1, "旧 Browser 记录应从磁盘清除");
    assert_eq!(saved.nodes[0].id, terminal, "磁盘应保留 Terminal 身份");
    assert!(
        !nodes.iter().any(|node| node.id == browser),
        "旧 Browser 不得出现在首页预览"
    );
}
