use super::{DatabaseMode, DatabasePaths};
use crate::ProjectStore;
use std::path::Path;

/// DP-1：同一 HOME 下 Production 与本地 Test 必须解析到物理隔离的全局和项目数据库。
///
/// 数据构造（含关键数值的推导过程）：
///   home = /Users/tester
///   workspace = /work/project
///   数据库模式数 = Production(1) + Test(1) = 2
///   每种模式路径数 = app DB(1) + project DB(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用相同 HOME 构造 Production 路径 → 全局库进入 `Norma OS`。
///   2. 用相同 HOME 构造 Test 路径 → 全局库进入 `Norma OS Test`。
///   3. 分别解析同一 workspace 的项目库 → 使用不同文件名。
///
/// 预期结果：
///   - 正断言：Production 使用既有 `app.sqlite` 和 `project.sqlite`。
///   - 正断言：Test 使用独立 `app.sqlite` 和 `project.test.sqlite`。
///   - 负断言：两种模式的全局库与项目库路径均不得相等。
#[test]
fn production_and_local_test_database_paths_are_physically_isolated() {
    let home = Path::new("/Users/tester");
    let workspace = Path::new("/work/project");

    let production = DatabasePaths::production_for_home(home);
    let test = DatabasePaths::local_test_for_home(home);

    assert_eq!(
        production.mode(),
        DatabaseMode::Production,
        "Production 路径必须保留明确的 Production 模式"
    );
    assert_eq!(
        production.app_db_path(),
        Path::new("/Users/tester/Library/Application Support/Norma OS/app.sqlite"),
        "Production 全局数据库必须保持现有 Norma OS/app.sqlite 路径"
    );
    assert_eq!(
        production.project_db_path(workspace),
        Path::new("/work/project/.multisoul/project.sqlite"),
        "Production 项目数据库必须保持现有 project.sqlite 文件名"
    );
    assert_eq!(
        test.mode(),
        DatabaseMode::Test,
        "本地开发路径必须明确标记为 Test 模式"
    );
    assert_eq!(
        test.app_db_path(),
        Path::new("/Users/tester/Library/Application Support/Norma OS Test/app.sqlite"),
        "本地 Test 全局数据库必须位于独立的 Norma OS Test 目录"
    );
    assert_eq!(
        test.project_db_path(workspace),
        Path::new("/work/project/.multisoul/project.test.sqlite"),
        "本地 Test 项目数据库必须使用 project.test.sqlite"
    );
    assert_ne!(
        production.app_db_path(),
        test.app_db_path(),
        "Production 与 Test 全局数据库不得解析到同一物理文件"
    );
    assert_ne!(
        production.project_db_path(workspace),
        test.project_db_path(workspace),
        "Production 与 Test 项目数据库不得解析到同一物理文件"
    );
}

/// DP-2：自动化测试显式提供数据根时，全局库和临时 workspace 项目库必须留在测试边界内。
///
/// 数据构造（含关键数值的推导过程）：
///   data root = /tmp/norma-case-1
///   workspace = /tmp/norma-case-1/workspace
///   允许的数据库文件数 = app DB(1) + project DB(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用测试用例的临时数据根构造隔离路径。
///   2. 直接取得 app DB 路径。
///   3. 对临时 workspace 解析项目 DB 路径。
///
/// 预期结果：
///   - 正断言：app DB 位于测试用例临时根。
///   - 正断言：项目 DB 使用 `project.test.sqlite`。
///   - 负断言：任何路径都不应落到 Production 的 `Norma OS` 或 `project.sqlite`。
#[test]
fn isolated_test_paths_stay_under_the_explicit_test_boundary() {
    let data_root = Path::new("/tmp/norma-case-1");
    let workspace = data_root.join("workspace");

    let paths = DatabasePaths::isolated_test(data_root);

    assert_eq!(
        paths.mode(),
        DatabaseMode::Test,
        "显式自动化测试路径必须保持 Test 模式"
    );
    assert_eq!(
        paths.app_db_path(),
        Path::new("/tmp/norma-case-1/app.sqlite"),
        "自动化测试 app DB 必须直接位于显式临时数据根"
    );
    assert_eq!(
        paths.project_db_path(&workspace),
        Path::new("/tmp/norma-case-1/workspace/.multisoul/project.test.sqlite"),
        "自动化测试项目库必须位于临时 workspace 且使用测试文件名"
    );
    assert!(
        !paths
            .app_db_path()
            .to_string_lossy()
            .contains("Application Support/Norma OS/"),
        "自动化测试 app DB 不得落入 Production Application Support 目录"
    );
    assert!(
        !paths
            .project_db_path(&workspace)
            .ends_with("project.sqlite"),
        "自动化测试项目库不得回退到 Production 的 project.sqlite"
    );
}

/// DP-3：已有测试 SettingsStore 文件必须能成为同一 App 实例的唯一全局数据库。
///
/// 数据构造（含关键数值的推导过程）：
///   settings DB = /tmp/norma-case-2/custom-settings.sqlite
///   workspace = /tmp/norma-case-2/workspace
///   显式数据库文件数 = app DB(1) + project DB(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用已有 settings DB 文件路径构造 Test 路径集合。
///   2. 读取 app DB 路径，确认没有被重写为固定 `app.sqlite`。
///   3. 为 workspace 解析项目数据库路径。
///
/// 预期结果：
///   - 正断言：app DB 精确保留调用方提供的文件路径。
///   - 正断言：项目库仍使用 `project.test.sqlite`。
///   - 负断言：模式不得因自定义文件名回退到 Production。
#[test]
fn isolated_test_database_preserves_the_explicit_app_database_file() {
    let app_db = Path::new("/tmp/norma-case-2/custom-settings.sqlite");
    let workspace = Path::new("/tmp/norma-case-2/workspace");

    let paths = DatabasePaths::isolated_test_database(app_db);

    assert_eq!(
        paths.mode(),
        DatabaseMode::Test,
        "显式测试数据库文件必须保持 Test 模式"
    );
    assert_eq!(
        paths.app_db_path(),
        app_db,
        "显式测试数据库文件名不得被重写为 app.sqlite"
    );
    assert_eq!(
        paths.project_db_path(workspace),
        Path::new("/tmp/norma-case-2/workspace/.multisoul/project.test.sqlite"),
        "显式测试 app DB 仍必须配套使用 project.test.sqlite"
    );
    assert_ne!(
        paths.mode(),
        DatabaseMode::Production,
        "自定义测试数据库文件不得回退到 Production 模式"
    );
}

/// DP-4：Test ProjectStore migration 只能创建测试项目库，不能触碰 Production 文件。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace = temp/workspace，已存在目录
///   Production project DB = .multisoul/project.sqlite，初始不存在
///   Test project DB = .multisoul/project.test.sqlite，初始不存在
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用隔离 Test 路径和临时 workspace 构造 ProjectStore。
///   2. 对 ProjectStore 执行现有 Schema migration。
///   3. 检查 Test 与 Production 两个候选数据库文件。
///
/// 预期结果：
///   - 正断言：ProjectStore 指向并创建 `project.test.sqlite`。
///   - 负断言：同一 workspace 的 `project.sqlite` 不得被创建。
#[test]
fn test_project_store_migration_never_creates_the_production_project_db() {
    let temp = tempfile::tempdir().expect("测试应创建隔离根目录");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).expect("测试应创建临时 workspace");
    let paths = DatabasePaths::isolated_test(temp.path());
    let production_db = workspace.join(".multisoul").join("project.sqlite");
    let test_db = workspace.join(".multisoul").join("project.test.sqlite");

    let store =
        ProjectStore::for_database_paths(&paths, &workspace).expect("Test ProjectStore 应构造成功");
    store.migrate().expect("Test project DB migration 应成功");

    assert_eq!(
        store.path(),
        test_db,
        "Test ProjectStore 必须指向 project.test.sqlite"
    );
    assert!(
        test_db.is_file(),
        "Test project migration 必须创建 project.test.sqlite"
    );
    assert!(
        !production_db.exists(),
        "Test project migration 不得创建 Production project.sqlite"
    );
}

/// DP-5：Test 项目数据库不可初始化时必须报告模式和目标文件，不能回退 Production。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace = temp/workspace，真实目录
///   blocked path = workspace/.multisoul，类型为普通文件
///   Test project DB = .multisoul/project.test.sqlite
///   Production project DB = .multisoul/project.sqlite
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用普通文件占用 `.multisoul` 目录位置。
///   2. 通过 Test DatabasePaths 初始化 ProjectStore。
///   3. 检查错误信息与 Production 候选文件。
///
/// 预期结果：
///   - 正断言：初始化错误包含 Test 模式和 `project.test.sqlite` 完整路径。
///   - 负断言：失败时不得改用或创建 `project.sqlite`。
#[test]
fn test_project_store_initialization_failure_reports_test_path_without_fallback() {
    let temp = tempfile::tempdir().expect("测试应创建隔离根目录");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).expect("测试应创建临时 workspace");
    let blocked_multisoul = workspace.join(".multisoul");
    std::fs::write(&blocked_multisoul, b"not-a-directory")
        .expect("测试应以普通文件阻塞 .multisoul 目录");
    let paths = DatabasePaths::isolated_test(temp.path());
    let test_db = paths.project_db_path(&workspace);
    let production_db = workspace.join(".multisoul").join("project.sqlite");

    let error = ProjectStore::initialize_for_database_paths(&paths, &workspace)
        .expect_err("不可初始化的 Test 项目数据库必须明确失败");
    let message = error.to_string();

    assert!(
        message.contains("Test"),
        "项目初始化错误必须包含 Test 模式，实际: {message}"
    );
    assert!(
        message.contains(test_db.to_string_lossy().as_ref()),
        "项目初始化错误必须包含 project.test.sqlite 完整路径，实际: {message}"
    );
    assert!(
        !production_db.exists(),
        "Test 项目初始化失败不得创建 Production project.sqlite"
    );
}
