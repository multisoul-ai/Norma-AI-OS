use super::{database_isolation_support::rect_snapshot, MultisoulApp};
use canvas::{CanvasNodeKind, CanvasRect, RectNode};
use gpui::TestAppContext;
use storage::{DatabasePaths, ProjectStore, SettingsStore};
use terminal::{PtyBackend, TestPtyBackend};

const THEME_SETTING_KEY: &str = "appearance.theme.v1";

fn echo_factory() -> runtime::PtyFactory {
    Box::new(|_cmd, _cols, _rows, tx| Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>))
}

/// DBI-0：每个自动化 App 实例都必须获得独占临时数据库根，并由 Guard 回收。
///
/// 数据构造（含关键数值的推导过程）：
///   自动化数据库实例数 = first(1) + second(1) = 2
///   每个实例 app DB 数 = 1
///   预期唯一数据库路径数 = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 连续创建两个自动化测试数据库生命周期对象。
///   2. 比较两个 app DB 路径，并检查各自父目录已经创建。
///   3. 释放 first Guard，检查仅 first 根被删除，second 根仍存在。
///   4. 释放 second Guard，检查 second 根也被删除。
///
/// 预期结果：
///   - 正断言：两个实例拥有不同且已创建的临时根。
///   - 正断言：释放各自 Guard 后，对应临时根被删除。
///   - 负断言：释放 first Guard 不得删除仍存活的 second 根。
#[test]
fn test_isolated_automation_database_lifetime_is_unique_and_guarded() {
    let (first_paths, first_guard) =
        crate::database::isolated_test_database().expect("first 测试数据库根应创建成功");
    let (second_paths, second_guard) =
        crate::database::isolated_test_database().expect("second 测试数据库根应创建成功");
    let first_root = first_paths
        .app_db_path()
        .parent()
        .expect("first app DB 应有临时父目录")
        .to_path_buf();
    let second_root = second_paths
        .app_db_path()
        .parent()
        .expect("second app DB 应有临时父目录")
        .to_path_buf();

    assert_ne!(
        first_paths.app_db_path(),
        second_paths.app_db_path(),
        "不同自动化 App 实例不得共享 app DB"
    );
    assert!(
        first_root.is_dir(),
        "first 自动化数据库临时根必须在构造时创建"
    );
    assert!(
        second_root.is_dir(),
        "second 自动化数据库临时根必须在构造时创建"
    );

    drop(first_guard);

    assert!(
        !first_root.exists(),
        "释放 first Guard 后必须回收 first 临时数据库根"
    );
    assert!(
        second_root.is_dir(),
        "释放 first Guard 不得删除仍由 second Guard 持有的根"
    );

    drop(second_guard);

    assert!(
        !second_root.exists(),
        "释放 second Guard 后必须回收 second 临时数据库根"
    );
}

/// DBI-0B：运行模式只能由打包编译标记决定，本地构建默认进入 Test。
///
/// 数据构造（含关键数值的推导过程）：
///   home = /Users/tester
///   compile marker cases = disabled(1) + enabled(1) = 2
///   预期模式 = Test(1) + Production(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对相同 HOME 关闭 production-data 标记解析路径。
///   2. 对相同 HOME 开启 production-data 标记解析路径。
///   3. 比较两种模式和 app DB 物理路径。
///
/// 预期结果：
///   - 正断言：未带标记的 cargo run 语义使用固定 Test 路径。
///   - 正断言：带标记的正式打包语义使用 Production 路径。
///   - 负断言：两种编译模式不得解析到同一 app DB。
#[test]
fn test_compile_marker_selects_production_while_local_build_defaults_to_test() {
    let home = std::path::Path::new("/Users/tester");

    let local = crate::database::database_paths_for_home(home, false);
    let packaged = crate::database::database_paths_for_home(home, true);

    assert_eq!(
        local.mode(),
        storage::DatabaseMode::Test,
        "没有 production-data 标记的本地构建必须使用 Test 模式"
    );
    assert_eq!(
        local.app_db_path(),
        std::path::Path::new("/Users/tester/Library/Application Support/Norma OS Test/app.sqlite"),
        "本地 cargo run 必须使用固定 Norma OS Test/app.sqlite"
    );
    assert_eq!(
        packaged.mode(),
        storage::DatabaseMode::Production,
        "正式打包标记必须选择 Production 模式"
    );
    assert_eq!(
        packaged.app_db_path(),
        std::path::Path::new("/Users/tester/Library/Application Support/Norma OS/app.sqlite"),
        "正式打包必须保持既有 Production app.sqlite 路径"
    );
    assert_ne!(
        local.app_db_path(),
        packaged.app_db_path(),
        "本地构建与正式打包不得共享 app DB"
    );
}

/// DBI-0D：独立进程测试根只能覆盖 Test 路径，不能把 Production 重定向。
///
/// 数据构造（含关键数值的推导过程）：
///   home = /Users/tester
///   L3 test root = /tmp/norma-l3
///   mode cases = Test(1) + Production(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Test 模式携带显式 L3 根解析数据库路径。
///   2. Production 模式携带相同 L3 根解析数据库路径。
///   3. 分别与 L3 临时路径和正式 Application Support 路径比较。
///
/// 预期结果：
///   - 正断言：Test 模式 app DB 位于 L3 临时根。
///   - 正断言：Production 仍使用正式路径。
///   - 负断言：L3 测试根不得改变 Production app DB。
#[test]
fn test_external_test_root_overrides_only_test_mode() {
    let home = std::path::Path::new("/Users/tester");
    let test_root = std::path::Path::new("/tmp/norma-l3");

    let test = crate::database::database_paths_for_home_and_test_root(home, false, Some(test_root))
        .expect("绝对 L3 测试根应可解析");
    let production =
        crate::database::database_paths_for_home_and_test_root(home, true, Some(test_root))
            .expect("Production 应忽略 L3 测试根");

    assert_eq!(
        test.app_db_path(),
        std::path::Path::new("/tmp/norma-l3/app.sqlite"),
        "独立 L3 Test 进程必须能把 app DB 放入显式临时根"
    );
    assert_eq!(
        production.app_db_path(),
        std::path::Path::new("/Users/tester/Library/Application Support/Norma OS/app.sqlite"),
        "Production 必须忽略测试根并保持正式路径"
    );
    assert_ne!(
        production.app_db_path(),
        test.app_db_path(),
        "L3 测试根不得把 Production 与 Test 重定向到同一数据库"
    );
}

/// DBI-0E：外部 Test 根不得直接或经符号链接解析到 Production 数据目录。
///
/// 数据构造（含关键数值的推导过程）：
///   home = temp/home
///   Production root = home/Library/Application Support/Norma OS
///   冲突入口 = direct root(1) + symlink alias(1) + symlink/..(1) = 3
///   三个入口最终候选 app DB 均 = Production root/app.sqlite
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Production 数据目录、child 目录和两条绝对符号链接。
///   2. 分别把 Production 目录、直接符号链接、`child` 链接后的 `..` 作为 Test 根。
///   3. 检查三次解析都明确失败，且错误保留各自的调用方路径。
///
/// 预期结果：
///   - 正断言：直接根、符号链接根与链接后的 `..` 都返回 InvalidTestDataRoot。
///   - 负断言：任一路径都不得生成指向 Production app.sqlite 的 Test DatabasePaths。
#[test]
fn test_external_test_root_rejects_direct_and_symlinked_production_directory() {
    let temp = tempfile::tempdir().expect("测试应创建隔离 HOME");
    let home = temp.path().join("home");
    let production = DatabasePaths::production_for_home(&home);
    let production_root = production
        .app_db_path()
        .parent()
        .expect("Production app DB 必须有数据目录");
    std::fs::create_dir_all(production_root).expect("测试应创建 Production 数据目录");
    let alias = temp.path().join("production-alias");
    std::os::unix::fs::symlink(production_root, &alias)
        .expect("测试应创建指向 Production 数据目录的符号链接");
    let production_child = production_root.join("child");
    std::fs::create_dir(&production_child).expect("测试应创建 Production child 目录");
    let child_alias = temp.path().join("production-child-alias");
    std::os::unix::fs::symlink(&production_child, &child_alias)
        .expect("测试应创建指向 Production child 的符号链接");
    let symlink_parent = child_alias.join("..");

    let direct_error =
        crate::database::database_paths_for_home_and_test_root(&home, false, Some(production_root))
            .expect_err("Production 数据目录不得被接受为 Test 根");
    let symlink_error =
        crate::database::database_paths_for_home_and_test_root(&home, false, Some(&alias))
            .expect_err("指向 Production 数据目录的符号链接不得被接受为 Test 根");
    let symlink_parent_error =
        crate::database::database_paths_for_home_and_test_root(&home, false, Some(&symlink_parent))
            .expect_err("符号链接后接 .. 解析到 Production 数据目录时不得被接受为 Test 根");

    assert!(
        matches!(
            direct_error,
            storage::StorageError::InvalidTestDataRoot(ref root) if root == production_root
        ),
        "直接 Production 根必须返回携带原路径的 InvalidTestDataRoot"
    );
    assert!(
        matches!(
            symlink_error,
            storage::StorageError::InvalidTestDataRoot(ref root) if root == &alias
        ),
        "Production 符号链接必须返回携带 alias 的 InvalidTestDataRoot"
    );
    assert!(
        matches!(
            symlink_parent_error,
            storage::StorageError::InvalidTestDataRoot(ref root) if root == &symlink_parent
        ),
        "Production child 符号链接后的 .. 必须返回携带原路径的 InvalidTestDataRoot"
    );
}

/// DBI-0C：cargo-packager 必须显式为主 App 开启唯一的 Production 数据标记。
///
/// 数据构造（含关键数值的推导过程）：
///   Packager 配置文件数 = 1
///   主 App release build 命令数 = 1
///   必需 feature = production-data
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 编译期读取仓库根 `Packager.toml`。
///   2. 归一化可选的 --no-default-features 参数，检查主 App 的显式 production-data feature。
///   3. 同时检查旧的无 feature 主 App 构建片段已经消失。
///
/// 预期结果：
///   - 正断言：打包命令包含 `--features production-data`。
///   - 负断言：不得保留无 Production 标记的主 App release 构建。
#[test]
fn test_packager_enables_the_production_data_feature_for_the_main_app() {
    // --no-default-features 只关闭默认 feature，不改变显式 production-data 的含义。
    let packager = include_str!("../../../../Packager.toml").replace("--no-default-features ", "");

    assert!(
        packager.contains("cargo build --release -p app --features production-data"),
        "Packager 主 App 构建必须显式开启 production-data"
    );
    assert!(
        !packager.contains("cargo build --release -p app &&"),
        "Packager 不得继续使用缺少 production-data 的主 App 构建命令"
    );
}

/// DBI-1：显式 Test DatabasePaths 构造 App 时，所有启动设置必须来自 Test app DB。
///
/// 数据构造（含关键数值的推导过程）：
///   Production app DB = production/app.sqlite，主题为 `sapphire-night`
///   Test app DB = test/app.sqlite，主题为 `forest-mint`
///   App launches = 1，候选数据库 = Production(1) + Test(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别向 Production 与 Test SettingsStore 写入不同主题哨兵。
///   2. 用显式 Test DatabasePaths 和测试 PTY 构造 headless MultisoulApp。
///   3. 通过公开 Settings snapshot 读取启动后的有效主题。
///   4. 比较 Production SQLite 原始字节并重新读取主题，确认启动过程未改写它。
///
/// 预期结果：
///   - 正断言：App 启动主题为 Test 哨兵 `forest-mint`。
///   - 正断言：Production 哨兵仍为 `sapphire-night`。
///   - 负断言：App 不得加载 Production 哨兵，Production 文件字节不得变化。
#[test]
fn test_app_bootstrap_reads_only_the_explicit_test_app_database() {
    let temp = tempfile::tempdir().expect("测试应创建隔离数据根");
    let production_paths = DatabasePaths::production_for_home(&temp.path().join("production"));
    let test_paths = DatabasePaths::isolated_test(&temp.path().join("test"));
    let production_store = SettingsStore::new(production_paths.app_db_path().to_path_buf());
    let test_store = SettingsStore::new(test_paths.app_db_path().to_path_buf());
    production_store
        .set_string(THEME_SETTING_KEY, "sapphire-night")
        .expect("测试应写入 Production 主题哨兵");
    test_store
        .set_string(THEME_SETTING_KEY, "forest-mint")
        .expect("测试应写入 Test 主题哨兵");
    let production_bytes_before = std::fs::read(production_paths.app_db_path())
        .expect("App 启动前应读取 Production SQLite 原始字节");

    let mut test_cx = TestAppContext::single();
    let (app, cx) = test_cx.add_window_view(|_window, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            echo_factory(),
            test_paths.clone(),
        )
    });
    let snapshot = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    let production_bytes_after = std::fs::read(production_paths.app_db_path())
        .expect("App 启动后应读取 Production SQLite 原始字节");
    let production_after = production_store
        .get_string(THEME_SETTING_KEY)
        .expect("Production 主题哨兵应保持可读");

    assert_eq!(
        snapshot.active_theme_id, "forest-mint",
        "显式 Test DatabasePaths 构造的 App 必须从 Test app DB 加载主题"
    );
    assert_ne!(
        snapshot.active_theme_id, "sapphire-night",
        "显式 Test DatabasePaths 构造的 App 不得读取 Production 主题哨兵"
    );
    assert_eq!(
        production_after.as_deref(),
        Some("sapphire-night"),
        "Test App 启动后 Production 主题必须保持原值"
    );
    assert_ne!(
        production_after.as_deref(),
        Some("forest-mint"),
        "Test App 启动不得把 Test 主题写入 Production app DB"
    );
    assert_eq!(
        production_bytes_after, production_bytes_before,
        "Test App 启动前后 Production app DB 文件字节必须完全不变"
    );
}

/// DBI-2：显式 Test App 进入 workspace 后只能绑定并迁移 `project.test.sqlite`。
///
/// 数据构造（含关键数值的推导过程）：
///   Test app DB = temp/data/app.sqlite
///   workspace = temp/workspace
///   候选项目库 = project.sqlite(Production) + project.test.sqlite(Test) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用显式 Test DatabasePaths 构造 headless App。
///   2. 通过公开 `enter_canvas_for_test` 进入临时 workspace。
///   3. 检查 `.multisoul` 下两个候选项目数据库文件。
///
/// 预期结果：
///   - 正断言：Test 项目数据库被创建并可用于后续持久化。
///   - 负断言：Production `project.sqlite` 不得被创建。
#[test]
fn test_app_workspace_binding_uses_only_the_test_project_database() {
    let temp = tempfile::tempdir().expect("测试应创建隔离根目录");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).expect("测试应创建临时 workspace");
    let test_paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let production_db = workspace.join(".multisoul").join("project.sqlite");
    let test_db = workspace.join(".multisoul").join("project.test.sqlite");

    let mut test_cx = TestAppContext::single();
    let (app, cx) = test_cx.add_window_view(|_window, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            echo_factory(),
            test_paths.clone(),
        )
    });
    app.update(cx, |app, cx| app.enter_canvas_for_test(&workspace, cx));

    assert!(
        test_db.is_file(),
        "显式 Test App 进入 workspace 后必须创建 project.test.sqlite"
    );
    assert!(
        !production_db.exists(),
        "显式 Test App 进入 workspace 后不得创建 Production project.sqlite"
    );
}

/// DBI-3：真实 workspace 打开与保存必须恢复 Test 哨兵并保持 Production 快照不变。
///
/// 数据构造（含关键数值的推导过程）：
///   Test app DB = temp/data/app.sqlite
///   workspace = temp/workspace
///   Production snapshot = Rect node_id=101, color=0x111111
///   Test snapshot = Rect node_id=202, color=0x222222
///   App 新增 Test 节点 = 1，因此保存后 Test 节点数 = 1 + 1 = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别向 `project.sqlite` 与 `project.test.sqlite` 写入不同快照哨兵。
///   2. 用显式 Test DatabasePaths 构造 App，并走真实 workspace 打开流程。
///   3. 检查 Runtime 只恢复 node_id=202，不恢复 node_id=101。
///   4. 向 Runtime 新增一个 Rect、标记 Canvas dirty 并同步 flush。
///   5. 分别重读 Production 与 Test 快照。
///
/// 预期结果：
///   - 正断言：Runtime 恢复 Test 哨兵且保存后 Test 快照包含 2 个节点。
///   - 正断言：Production 快照仍只包含 node_id=101。
///   - 负断言：Runtime/Test 快照不得出现 Production node_id=101，Production 不得出现新增节点。
#[test]
fn test_real_workspace_open_flow_uses_the_app_test_database_paths() {
    let temp = tempfile::tempdir().expect("测试应创建隔离根目录");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).expect("测试应创建临时 workspace");
    let test_paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let production_paths = DatabasePaths::production_for_home(temp.path());
    let production_store = ProjectStore::for_database_paths(&production_paths, &workspace)
        .expect("Production ProjectStore 哨兵应构造成功");
    let test_store = ProjectStore::for_database_paths(&test_paths, &workspace)
        .expect("Test ProjectStore 哨兵应构造成功");
    production_store
        .canvas_store()
        .save_snapshot(&rect_snapshot(101, 0x111111))
        .expect("Production 项目快照哨兵应写入成功");
    test_store
        .canvas_store()
        .save_snapshot(&rect_snapshot(202, 0x222222))
        .expect("Test 项目快照哨兵应写入成功");

    let mut test_cx = TestAppContext::single();
    let (app, cx) = test_cx.add_window_view(|_window, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            echo_factory(),
            test_paths.clone(),
        )
    });
    cx.update(|window, app_context| {
        app.update(app_context, |app, app_context| {
            app.open_workspace_path(workspace.clone(), window, app_context);
        });
    });
    let (has_test_sentinel, has_production_sentinel) = app.update(cx, |app, _cx| {
        (
            app.runtime.document.node(202).is_some(),
            app.runtime.document.node(101).is_some(),
        )
    });
    app.update(cx, |app, _cx| {
        app.runtime.document.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0x333333 }),
            CanvasRect::new(30.0, 40.0, 120.0, 90.0),
        );
        app.mark_canvas_dirty();
        app.flush_persistence_now();
    });
    let production_after = production_store
        .canvas_store()
        .load_snapshot()
        .expect("保存后 Production 快照应可读取")
        .expect("Production 快照哨兵不得消失");
    let test_after = test_store
        .canvas_store()
        .load_snapshot()
        .expect("保存后 Test 快照应可读取")
        .expect("Test 快照不得消失");

    assert!(
        has_test_sentinel,
        "真实 workspace 打开流程必须恢复 Test node_id=202"
    );
    assert!(
        !has_production_sentinel,
        "Test App 不得从 Production project.sqlite 恢复 node_id=101"
    );
    assert_eq!(
        test_after.nodes.len(),
        2,
        "Test 保存后必须保留 node_id=202 并新增 1 个节点"
    );
    assert!(
        test_after.nodes.iter().any(|node| node.id == 202),
        "Test 保存后必须继续包含 Test 哨兵 node_id=202"
    );
    assert!(
        test_after.nodes.iter().all(|node| node.id != 101),
        "Test project.test.sqlite 不得混入 Production node_id=101"
    );
    assert_eq!(
        production_after.nodes.len(),
        1,
        "Test App 保存不得改变 Production 快照节点数"
    );
    assert_eq!(
        production_after.nodes[0].id, 101,
        "Production project.sqlite 必须保持原 node_id=101 哨兵"
    );
}
