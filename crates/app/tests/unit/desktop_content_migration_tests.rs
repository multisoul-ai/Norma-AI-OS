use super::*;
use canvas::{BrowserSpec, BrowserSurfaceKind, CanvasNodeKind, CanvasRect, Document, EgoLiteSpec};

/// 桌面项目后台加载时应在创建 Runtime 前持久清理旧内容卡片。
///
/// 数据构造：一个项目画布保存普通 Browser、Preview、EgoLite Browser 和 Terminal。
/// 执行过程：经正式 PreparedProject 桌面加载入口读取，再直接读取磁盘快照。
/// 预期结果：内存和磁盘均只保留 Ego、Terminal；项目文件和 Ego 任务描述不被删除。
#[test]
fn desktop_loader_prunes_ordinary_content_before_runtime_restore() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let file = root.join("plan.md");
    std::fs::write(&file, "# Plan").expect("应写入方案文件");
    let catalog = storage::DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
    let workspace = catalog.create("空间").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应创建画布区域");
    let project = ProjectStore::initialize_for_desktop_area(&paths, &root, &workspace.id, &area.id)
        .expect("应初始化画布存储");
    let mut document = Document::new();
    let bounds = CanvasRect::new(0.0, 0.0, 400.0, 300.0);
    document.add_node(
        CanvasNodeKind::Browser(BrowserSpec {
            url: "https://example.test".to_string(),
            title: None,
            surface: BrowserSurfaceKind::Native,
        }),
        bounds,
    );
    document.add_node(
        CanvasNodeKind::Preview(canvas::PreviewSpec {
            path: file.to_string_lossy().into_owned(),
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
    project
        .canvas_store()
        .save_snapshot(&document.to_snapshot())
        .expect("应保存旧画布");

    let prepared = PreparedProject::load_for_desktop(paths, area).expect("桌面项目应可加载");
    let loaded = prepared.document.expect("加载后应有画布");
    let persisted = project
        .canvas_store()
        .load_snapshot()
        .expect("应读取保存画布")
        .expect("保存画布应存在");
    let ids: Vec<_> = loaded.nodes.iter().map(|node| node.id).collect();
    let saved_ids: Vec<_> = persisted.nodes.iter().map(|node| node.id).collect();
    assert_eq!(
        ids,
        vec![ego, terminal],
        "运行时只能看到 Ego 与 Terminal 节点"
    );
    assert_eq!(saved_ids, ids, "旧内容节点必须从持久画布同步清除");
    assert!(file.exists(), "旧 Preview 的本地文件不得删除");
    assert!(
        matches!(
            &loaded.nodes[0].kind,
            CanvasNodeKind::Browser(BrowserSpec {
                surface: BrowserSurfaceKind::EgoLite(_),
                ..
            })
        ),
        "保留的 Browser 必须属于 Ego 任务"
    );
}
