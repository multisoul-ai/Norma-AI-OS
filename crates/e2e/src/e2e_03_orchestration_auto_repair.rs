//! Orchestration 自动修复：App 升级后在后台更新已授权 Provider 的受管组件。

use app::MultisoulApp;
use gpui::TestAppContext;
use norma::orchestration::{
    AggregateStatus, AuthorizationRegistry, InstallPaths, OrchestrationInstaller,
    OrchestrationProvider,
};
use std::fs;

struct StoreAuthorizationRegistry {
    store: storage::OrchestrationAuthorizationStore,
}

impl AuthorizationRegistry for StoreAuthorizationRegistry {
    type Error = storage::StorageError;

    fn authorize(
        &mut self,
        provider: OrchestrationProvider,
        token: &str,
        installed_version: &str,
    ) -> Result<(), Self::Error> {
        self.store
            .authorize(provider.slug(), token, installed_version)
    }

    fn revoke(&mut self, provider: OrchestrationProvider) -> Result<(), Self::Error> {
        self.store.revoke(provider.slug())
    }

    fn is_authorized(
        &self,
        provider: OrchestrationProvider,
        token: &str,
    ) -> Result<bool, Self::Error> {
        self.store.is_authorized(provider.slug(), token)
    }
}

fn write_bundle(paths: &InstallPaths, cli: &[u8], hook: &[u8]) {
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("E2E bundled CLI 必须有父目录"),
    )
    .expect("E2E 必须创建 bundle 目录");
    fs::write(&paths.bundled_cli, cli).expect("E2E 必须写入 bundled CLI");
    fs::write(&paths.bundled_hook, hook).expect("E2E 必须写入 bundled Hook");
}

/// 新 App 打开项目：已授权 Codex 的共享组件漂移应自动修复，无需用户点击 Repair。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider = Codex(已授权 1) + Claude/Cursor/OpenCode(未授权 3) + Grok(不支持 1)。
///   App 版本 = 当前 `CARGO_PKG_VERSION`（版本不变，仅模拟重新签名/构建产生字节漂移）。
///   共享二进制 = 2：CLI `cli-build-a`→`cli-build-b`，Hook `hook-build-a`→`hook-build-b`。
///   用户交互次数 = 0：不点击 Settings Repair。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用隔离 App SQLite 和旧 bundle 安装 Codex，确认初始四组件 Ready。
///   2. 将 bundle CLI/Hook 替换为新构建字节，确认安装器能复现 NeedsRepair。
///   3. 用隔离数据库和安装路径构造真实 MultisoulApp，经过 bootstrap 自动启动后台 reconcile。
///   4. 读取 Settings 公开快照和最终受管文件，不触发任何 Repair 点击。
///
/// 预期结果：
///   - 正断言：Codex 自动恢复 Ready，受管 CLI/Hook 精确等于当前 bundle 字节。
///   - 正断言：Codex 凭据逐字保留，授权仍有效。
///   - 负断言：未授权 Claude 不得变成 Ready，用户无需进入 Repair 流程。
#[test]
fn e2e_03_app_upgrade_auto_repairs_authorized_orchestration_without_click() {
    let temp = tempfile::tempdir().expect("Orchestration 自动修复 E2E 应创建临时目录");
    let database_paths = storage::DatabasePaths::isolated_test(&temp.path().join("database"));
    storage::AppStore::initialize(&database_paths).expect("E2E 必须初始化隔离 App 数据库");
    let database_path = database_paths.app_db_path().to_path_buf();
    let home = temp.path().join("home");
    let bundle = temp.path().join("bundle");
    let paths = InstallPaths::for_home_and_bundle_bin(&home, &bundle);
    write_bundle(&paths, b"cli-build-a", b"hook-build-a");
    let installer = OrchestrationInstaller::new(paths.clone(), env!("CARGO_PKG_VERSION"));
    let mut registry = StoreAuthorizationRegistry {
        store: storage::OrchestrationAuthorizationStore::new(database_path.clone()),
    };
    let installed = installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("旧 App 构建的 Codex 编排安装必须成功");
    let credential_path = paths.credential_path(OrchestrationProvider::Codex);
    let credential_before = fs::read_to_string(&credential_path).expect("Codex 凭据必须存在");
    assert_eq!(
        installed.aggregate_status,
        AggregateStatus::Ready,
        "初始 Codex 安装必须四组件全部 Ready"
    );

    write_bundle(&paths, b"cli-build-b", b"hook-build-b");
    let drifted = installer.inspect(OrchestrationProvider::Codex, &registry);
    assert_eq!(
        drifted.aggregate_status,
        AggregateStatus::NeedsRepair,
        "重新构建后的二进制字节漂移必须复现用户看到的 NeedsRepair"
    );

    let mut test_cx = TestAppContext::single();
    let startup_paths = paths.clone();
    let (app, cx) = test_cx.add_window_view(move |_window, cx| {
        MultisoulApp::new_with_database_paths_and_orchestration_paths(
            cx,
            database_paths,
            startup_paths,
        )
    });
    cx.run_until_parked();

    let snapshot = app.update(cx, |app, _cx| app.orchestration_settings_e2e_snapshot());
    let credential_after =
        fs::read_to_string(&credential_path).expect("自动修复后 Codex 凭据必须保留");
    assert_eq!(
        snapshot.providers[0].provider, "Codex",
        "Orchestration 快照第一个 Provider 必须是 Codex"
    );
    assert_eq!(
        snapshot.providers[0].status, "Ready",
        "App 后台 reconcile 完成后 Codex 必须自动恢复 Ready"
    );
    assert_eq!(
        snapshot.providers[1].provider, "Claude Code",
        "Orchestration 快照第二个 Provider 必须明确是 Claude Code"
    );
    assert_eq!(
        snapshot.providers[1].status, "Needs repair",
        "共享二进制存在但 Claude 未授权、Skill/Hook 缺失时必须精确显示 Needs repair"
    );
    assert_eq!(
        fs::read(paths.managed_cli_path()).expect("自动修复后受管 CLI 必须可读取"),
        b"cli-build-b",
        "受管 CLI 必须自动更新为当前 App bundle 字节"
    );
    assert_eq!(
        fs::read(paths.managed_hook_path()).expect("自动修复后受管 Hook 必须可读取"),
        b"hook-build-b",
        "受管 Hook 必须自动更新为当前 App bundle 字节"
    );
    assert_eq!(
        credential_after, credential_before,
        "自动修复不得轮换或重写仍然有效的 Codex 凭据"
    );
    assert!(
        registry
            .is_authorized(
                OrchestrationProvider::Codex,
                serde_json::from_str::<serde_json::Value>(&credential_after)
                    .expect("Codex 凭据必须是合法 JSON")["token"]
                    .as_str()
                    .expect("Codex 凭据必须包含字符串 Token")
            )
            .expect("自动修复后 Codex 授权校验必须可执行"),
        "自动修复后 Codex 原 Token 必须继续通过全局授权校验"
    );
}
