use super::{
    authorization_database_path, log_operation_error, missing_integration, OrchestrationOperation,
    OrchestrationSettingsState,
};
use norma::orchestration::OrchestrationProvider;
use std::{fs, sync::Mutex};

/// 编排授权存储：后台扫描必须沿用 App 已选定的 Test app DB。
///
/// 数据构造（含关键数值的推导过程）：
///   Production app DB = /tmp/production/app.sqlite
///   Test app DB = /tmp/test/app.sqlite
///   候选数据库数 = Production(1) + Test(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造显式 Test DatabasePaths。
///   2. 解析编排授权后台任务应捕获的数据库文件。
///   3. 与 Test 和 Production 两个候选路径分别比较。
///
/// 预期结果：
///   - 正断言：后台授权 Store 精确使用 Test app DB。
///   - 负断言：不得在线程内重新解析或回退到 Production app DB。
#[test]
fn orchestration_authorization_uses_selected_test_app_database() {
    let test_paths = storage::DatabasePaths::isolated_test(std::path::Path::new("/tmp/test"));
    let production_path = std::path::Path::new("/tmp/production/app.sqlite");

    let selected = authorization_database_path(&test_paths);

    assert_eq!(
        selected,
        std::path::Path::new("/tmp/test/app.sqlite"),
        "编排授权后台任务必须捕获已选定的 Test app DB"
    );
    assert_ne!(
        selected, production_path,
        "编排授权后台任务不得重新解析为 Production app DB"
    );
}

/// 后台扫描代际：安装/卸载开始前的旧扫描不得覆盖最新授权缓存。
///
/// 数据构造（含关键数值的推导过程）：
///   initial generation = 0
///   stale refresh      = begin_refresh() = 1
///   operation refresh  = begin_refresh() = 2
///   Total callbacks    = stale(1) + current(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动普通扫描并记录 generation=1。
///   2. 安装/卸载开始后推进到 generation=2。
///   3. generation=1 晚到 → 拒绝结果，调用方不得替换授权缓存。
///   4. generation=2 到达 → 接受结果，调用方可以同步替换授权缓存。
///
/// 预期结果：
///   - 正断言：当前 generation=2 被接受。
///   - 负断言：过期 generation=1 被拒绝，不能重新启用已撤销令牌。
#[test]
fn stale_refresh_generation_cannot_replace_authorization_cache() {
    let mut state = OrchestrationSettingsState::default();
    let stale_generation = state.begin_refresh();
    let current_generation = state.begin_refresh();
    let integrations = vec![missing_integration(OrchestrationProvider::Codex)];

    assert!(
        !state.apply_refresh(stale_generation, integrations.clone()),
        "generation=1 的旧扫描不得在 generation=2 后覆盖授权缓存"
    );
    assert!(
        state.apply_refresh(current_generation, integrations),
        "当前 generation=2 扫描必须被接受并允许同步授权缓存"
    );
    assert_ne!(
        stale_generation, current_generation,
        "旧扫描与当前操作必须使用不同 generation 才能隔离竞态"
    );
}

/// 安装器内部失败日志：UI 已收到的 operation_error 必须同时进入 tracing。
///
/// 数据构造（含关键数值的推导过程）：
///   failure reports = 1（Codex install_or_repair + os error 2）
///   success reports = 1（同 Provider、同操作，但 error=None）
///   expected warning events = failure(1) + success(0) = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 安装局部无 ANSI tracing subscriber，并把输出写入临时日志文件。
///   2. 上报一次 Codex install_or_repair 失败 → 应写 warning。
///   3. 上报一次无错误结果 → 不应伪造第二条失败 warning。
///   4. 读取日志并检查 Provider、操作和原始系统错误。
///
/// 预期结果：
///   - 正断言：日志包含唯一一条 orchestration operation failed。
///   - 正断言：日志包含 provider=codex 与 operation=install_or_repair。
///   - 正断言：日志保留 No such file or directory (os error 2)。
///   - 负断言：成功结果不得产生第二条失败日志。
#[test]
fn installer_operation_error_is_written_to_tracing_log() {
    let temp = tempfile::tempdir().expect("测试应创建隔离日志目录");
    let log_path = temp.path().join("orchestration.log");
    let log_file = fs::File::create(&log_path).expect("测试应创建日志文件");
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_writer(Mutex::new(log_file))
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        log_operation_error(
            OrchestrationProvider::Codex,
            OrchestrationOperation::InstallOrRepair,
            Some("No such file or directory (os error 2)"),
        );
        log_operation_error(
            OrchestrationProvider::Codex,
            OrchestrationOperation::InstallOrRepair,
            None,
        );
    });

    let contents = fs::read_to_string(log_path).expect("测试应读取 tracing 日志");
    assert!(
        contents.contains("orchestration operation failed"),
        "安装器返回 operation_error 时必须写入可检索的失败事件"
    );
    assert!(
        contents.contains("provider=\"codex\""),
        "失败日志必须包含 provider=codex 以定位受影响集成"
    );
    assert!(
        contents.contains("operation=\"install_or_repair\""),
        "失败日志必须区分 install_or_repair 与 uninstall"
    );
    assert!(
        contents.contains("No such file or directory (os error 2)"),
        "失败日志必须保留 UI 已收到的底层系统错误"
    );
    assert_eq!(
        contents.matches("orchestration operation failed").count(),
        1,
        "failure(1) + success(0) 应只产生一条失败日志，成功结果不得误报"
    );
}
