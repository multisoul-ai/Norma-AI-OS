use norma::orchestration::{
    AggregateStatus, AuthorizationRegistry, ComponentState, InstallPaths, OrchestrationInstaller,
    OrchestrationProvider, ProviderIntegration,
};
use std::collections::HashMap;
use std::fs;

#[derive(Default)]
struct MemoryAuthorizationRegistry {
    tokens: HashMap<OrchestrationProvider, String>,
    authorize_calls: usize,
}

impl AuthorizationRegistry for MemoryAuthorizationRegistry {
    type Error = String;

    fn authorize(
        &mut self,
        provider: OrchestrationProvider,
        token: &str,
        _installed_version: &str,
    ) -> Result<(), Self::Error> {
        self.authorize_calls = self.authorize_calls.saturating_add(1);
        self.tokens.insert(provider, token.to_string());
        Ok(())
    }

    fn revoke(&mut self, provider: OrchestrationProvider) -> Result<(), Self::Error> {
        self.tokens.remove(&provider);
        Ok(())
    }

    fn is_authorized(
        &self,
        provider: OrchestrationProvider,
        token: &str,
    ) -> Result<bool, Self::Error> {
        Ok(self
            .tokens
            .get(&provider)
            .is_some_and(|expected| expected == token))
    }
}

fn write_bundle(paths: &InstallPaths, cli: &[u8], hook: &[u8]) {
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundle CLI 必须有父目录"),
    )
    .expect("测试必须创建 bundle 目录");
    fs::write(&paths.bundled_cli, cli).expect("测试必须写入 bundled CLI");
    fs::write(&paths.bundled_hook, hook).expect("测试必须写入 bundled Hook");
}

fn integration(
    integrations: &[ProviderIntegration],
    provider: OrchestrationProvider,
) -> &ProviderIntegration {
    integrations
        .iter()
        .find(|integration| integration.provider == provider)
        .unwrap_or_else(|| panic!("reconcile 报告必须包含 Provider {provider:?}"))
}

/// App 升级自动修复：只更新已授权 Provider 的 Norma 受管组件，不重新授权或安装新 Provider。
///
/// 数据构造（含关键数值的推导过程）：
///   已安装 Provider = 2：Codex + Claude；未安装 Provider = 1：OpenCode。
///   旧版本 = 1.0.0；新版本 = 2.0.0。
///   共享二进制 = 2：CLI `cli-v1`→`cli-v2`，Hook `hook-v1`→`hook-v2`。
///   初始授权写入 = Codex(1) + Claude(1) = 2 次；自动修复新增授权预期 = 0 次。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用旧 bundle 安装 Codex 与 Claude，保留 Codex 凭据原文和授权调用数。
///   2. 将同一 bundle 的 CLI/Hook 替换为新字节，并用新版本安装器重新检查。
///   3. 调用 reconcile_installed，自动升级已授权 Provider 的 Skill、CLI 与 Hook。
///   4. 检查 Codex/Claude 最终状态、OpenCode 空状态、共享二进制和授权副作用。
///
/// 预期结果：
///   - 正断言：Codex 与 Claude 从 NeedsRepair 恢复为 Ready，共享二进制和 Skill 升至 v2。
///   - 正断言：Codex 凭据和总授权次数保持不变。
///   - 负断言：未授权的 OpenCode 不得生成 Skill/凭据或变成 Ready，reconcile 不得产生失败。
#[test]
fn app_upgrade_reconciles_authorized_providers_without_installing_new_provider() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let home = temp.path().join("home");
    let bundle = temp.path().join("bundle");
    let paths = InstallPaths::for_home_and_bundle_bin(&home, &bundle);
    write_bundle(&paths, b"cli-v1", b"hook-v1");
    let old_installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();
    old_installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("旧版本 Codex 安装必须成功");
    old_installer
        .install_or_repair(OrchestrationProvider::Claude, &mut registry)
        .expect("旧版本 Claude 安装必须成功");
    let codex_credential_path = paths.credential_path(OrchestrationProvider::Codex);
    let codex_credential_before =
        fs::read_to_string(&codex_credential_path).expect("旧版本 Codex 凭据必须存在");
    assert_eq!(
        registry.authorize_calls, 2,
        "Codex(1) + Claude(1) 初次安装必须精确写入两次授权"
    );

    write_bundle(&paths, b"cli-v2", b"hook-v2");
    let upgraded_installer = OrchestrationInstaller::new(paths.clone(), "2.0.0");
    let codex_before = upgraded_installer.inspect(OrchestrationProvider::Codex, &registry);
    let claude_before = upgraded_installer.inspect(OrchestrationProvider::Claude, &registry);
    let opencode_before = upgraded_installer.inspect(OrchestrationProvider::OpenCode, &registry);
    assert_eq!(
        codex_before.aggregate_status,
        AggregateStatus::NeedsRepair,
        "App 升级后旧 Codex 受管组件必须先被识别为 NeedsRepair"
    );
    assert_eq!(
        claude_before.aggregate_status,
        AggregateStatus::NeedsRepair,
        "App 升级后旧 Claude 受管组件必须先被识别为 NeedsRepair"
    );
    assert_eq!(
        opencode_before.aggregate_status,
        AggregateStatus::NeedsRepair,
        "共享 CLI 已存在时旧汇总器会把未安装 OpenCode 显示为 NeedsRepair"
    );

    let report = upgraded_installer.reconcile_installed(&mut registry);
    let codex_after = integration(&report.integrations, OrchestrationProvider::Codex);
    let claude_after = integration(&report.integrations, OrchestrationProvider::Claude);
    let opencode_after = integration(&report.integrations, OrchestrationProvider::OpenCode);
    let codex_credential_after =
        fs::read_to_string(&codex_credential_path).expect("自动修复后 Codex 凭据必须保留");
    let codex_skill = fs::read_to_string(&paths.codex_skill)
        .expect("自动修复后 Codex orchestration Skill 必须存在");
    let claude_skill = fs::read_to_string(&paths.claude_skill)
        .expect("自动修复后 Claude orchestration Skill 必须存在");

    assert!(
        report.failures.is_empty(),
        "合法的受管升级不应产生 reconcile 失败，actual={:?}",
        report.failures
    );
    assert_eq!(
        codex_after.aggregate_status,
        AggregateStatus::Ready,
        "Codex 受管组件自动升级后必须恢复 Ready"
    );
    assert_eq!(
        claude_after.aggregate_status,
        AggregateStatus::Ready,
        "Claude 受管组件自动升级后必须恢复 Ready"
    );
    assert_eq!(
        opencode_after.aggregate_status,
        AggregateStatus::NeedsRepair,
        "共享组件存在但 OpenCode 的 Skill、Hook 与授权缺失时必须精确保持 NeedsRepair"
    );
    assert_eq!(
        fs::read(paths.managed_cli_path()).expect("受管 CLI 必须可读取"),
        b"cli-v2",
        "全局受管 CLI 必须更新为当前 App bundle 字节"
    );
    assert_eq!(
        fs::read(paths.managed_hook_path()).expect("受管 Hook 必须可读取"),
        b"hook-v2",
        "全局受管 Hook 必须更新为当前 App bundle 字节"
    );
    assert!(
        codex_skill.contains("# managed-version: 2.0.0"),
        "Codex Skill 必须更新到当前 App 版本"
    );
    assert!(
        claude_skill.contains("# managed-version: 2.0.0"),
        "Claude Skill 必须更新到当前 App 版本"
    );
    assert_eq!(
        codex_credential_after, codex_credential_before,
        "自动修复不得轮换仍然有效的 Codex 凭据"
    );
    assert_eq!(
        registry.authorize_calls, 2,
        "自动修复只更新受管组件，不得重复写入或重新启用授权"
    );
    assert!(
        !paths.opencode_skill.exists(),
        "未授权 OpenCode 不得生成 orchestration Skill"
    );
    assert!(
        !paths
            .credential_path(OrchestrationProvider::OpenCode)
            .exists(),
        "未授权 OpenCode 不得生成 Provider 凭据"
    );
}

/// App 升级遇到用户文件冲突：自动 reconcile 必须保留原文并继续显示 NeedsRepair。
///
/// 数据构造（含关键数值的推导过程）：
///   已授权 Provider = 1：Cursor。
///   旧版本 = 1.0.0；新版本 = 2.0.0。
///   冲突文件 = Cursor orchestration rule，内容 `# user-owned cursor rule`。
///   初始授权写入 = 1 次；自动 reconcile 新增授权预期 = 0 次。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 完整安装旧版 Cursor，使凭据和授权均有效。
///   2. 用无 Norma marker 的用户 rule 替换受管 rule，并升级 bundle 字节与版本。
///   3. 调用 reconcile_installed，允许修复其他受管组件但跳过冲突 rule。
///   4. 读取最终 Cursor 状态、冲突文件、共享二进制和授权调用数。
///
/// 预期结果：
///   - 正断言：Cursor Skill 精确报告 Conflict，汇总保持 NeedsRepair。
///   - 正断言：其他受管二进制升级成功，授权调用数保持 1。
///   - 负断言：用户 rule 不得被覆盖，Cursor 不得错误显示 Ready。
#[test]
fn app_upgrade_preserves_user_conflict_for_manual_repair() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let home = temp.path().join("home");
    let bundle = temp.path().join("bundle");
    let paths = InstallPaths::for_home_and_bundle_bin(&home, &bundle);
    write_bundle(&paths, b"cli-v1", b"hook-v1");
    let old_installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();
    old_installer
        .install_or_repair(OrchestrationProvider::Cursor, &mut registry)
        .expect("旧版本 Cursor 安装必须成功");
    assert_eq!(
        registry.authorize_calls, 1,
        "Cursor 初次安装必须精确写入一次授权"
    );

    let user_rule = "# user-owned cursor rule\n";
    fs::write(&paths.cursor_rule, user_rule).expect("测试必须写入用户 Cursor rule");
    write_bundle(&paths, b"cli-v2", b"hook-v2");
    let upgraded_installer = OrchestrationInstaller::new(paths.clone(), "2.0.0");

    let report = upgraded_installer.reconcile_installed(&mut registry);
    let cursor = integration(&report.integrations, OrchestrationProvider::Cursor);
    let preserved =
        fs::read_to_string(&paths.cursor_rule).expect("自动 reconcile 后用户 Cursor rule 必须存在");

    assert!(
        report.failures.is_empty(),
        "用户文件冲突应作为可处理状态保留，不应伪装成安装器崩溃，actual={:?}",
        report.failures
    );
    assert!(
        matches!(
            cursor.skill_component,
            ComponentState::Conflict { ref path, .. } if path == &paths.cursor_rule
        ),
        "Cursor 自动 reconcile 后必须保留带精确路径的 Skill Conflict"
    );
    assert_eq!(
        cursor.aggregate_status,
        AggregateStatus::NeedsRepair,
        "未解决用户文件冲突时 Cursor 必须继续显示 NeedsRepair"
    );
    assert_ne!(
        cursor.aggregate_status,
        AggregateStatus::Ready,
        "自动 reconcile 不得把仍有用户冲突的 Cursor 错误显示为 Ready"
    );
    assert_eq!(
        preserved, user_rule,
        "自动 reconcile 不得覆盖无 Norma marker 的用户 Cursor rule"
    );
    assert!(
        !preserved.contains("norma-orchestration-managed"),
        "用户 Cursor rule 不得被静默替换为 Norma 受管内容"
    );
    assert_eq!(
        fs::read(paths.managed_cli_path()).expect("受管 CLI 必须可读取"),
        b"cli-v2",
        "存在用户 rule 冲突时其他 Norma 受管 CLI 仍应安全升级"
    );
    assert_eq!(
        fs::read(paths.managed_hook_path()).expect("受管 Hook 必须可读取"),
        b"hook-v2",
        "存在用户 rule 冲突时其他 Norma 受管 Hook 仍应安全升级"
    );
    assert_eq!(
        registry.authorize_calls, 1,
        "自动 reconcile 不得因用户文件冲突重新写入授权"
    );
}

/// OpenCode 插件冲突：用户插件保持人工处理，共享 Hook 二进制仍必须独立升级。
///
/// 数据构造（含关键数值的推导过程）：
///   已授权 Provider = 1：OpenCode。
///   旧/新 Hook = `hook-v1` / `hook-v2`；冲突插件 = `// user-owned plugin`。
///   共享 Hook 更新目标 = 1 次全局受管文件；用户插件覆盖次数预期 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 安装旧版 OpenCode，使 Skill、CLI、Hook 和授权全部 Ready。
///   2. 用无 Norma marker 的用户插件替换受管插件，并把 bundle 升级到 v2。
///   3. 调用 reconcile_installed，先处理共享二进制，再处理 Provider 专属配置。
///   4. 读取最终共享 Hook、用户插件与 OpenCode 汇总状态。
///
/// 预期结果：
///   - 正断言：共享 Hook 独立升级到 v2，OpenCode 插件仍精确报告 Conflict。
///   - 正断言：OpenCode 保持 NeedsRepair，等待用户处理冲突。
///   - 负断言：自动 reconcile 不得覆盖用户插件或把 OpenCode 错误显示 Ready。
#[test]
fn opencode_plugin_conflict_does_not_block_shared_hook_upgrade() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let home = temp.path().join("home");
    let bundle = temp.path().join("bundle");
    let paths = InstallPaths::for_home_and_bundle_bin(&home, &bundle);
    write_bundle(&paths, b"cli-v1", b"hook-v1");
    let old_installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();
    old_installer
        .install_or_repair(OrchestrationProvider::OpenCode, &mut registry)
        .expect("旧版本 OpenCode 安装必须成功");

    let user_plugin = "// user-owned plugin\n";
    fs::write(&paths.hook_paths.opencode_plugin, user_plugin)
        .expect("测试必须写入用户 OpenCode 插件");
    write_bundle(&paths, b"cli-v2", b"hook-v2");
    let upgraded_installer = OrchestrationInstaller::new(paths.clone(), "2.0.0");

    let report = upgraded_installer.reconcile_installed(&mut registry);
    let opencode = integration(&report.integrations, OrchestrationProvider::OpenCode);
    let preserved = fs::read_to_string(&paths.hook_paths.opencode_plugin)
        .expect("自动 reconcile 后用户 OpenCode 插件必须存在");

    assert!(
        matches!(
            opencode.hook_component,
            ComponentState::Conflict { ref path, .. }
                if path == &paths.hook_paths.opencode_plugin
        ),
        "OpenCode 用户插件必须保留带精确路径的 Hook Conflict"
    );
    assert_eq!(
        opencode.aggregate_status,
        AggregateStatus::NeedsRepair,
        "用户插件冲突未解决时 OpenCode 必须继续显示 NeedsRepair"
    );
    assert_ne!(
        opencode.aggregate_status,
        AggregateStatus::Ready,
        "共享 Hook 升级不得掩盖仍存在的 OpenCode 用户插件冲突"
    );
    assert_eq!(
        fs::read(paths.managed_hook_path()).expect("受管共享 Hook 必须可读取"),
        b"hook-v2",
        "OpenCode 插件冲突不得阻止 Norma 受管共享 Hook 独立升级"
    );
    assert_eq!(
        preserved, user_plugin,
        "自动 reconcile 不得覆盖无 Norma marker 的用户 OpenCode 插件"
    );
    assert!(
        !preserved.contains("multisoul-agent-hook"),
        "用户 OpenCode 插件不得被静默替换成 Norma 受管插件"
    );
}

/// 撤销授权：App 升级不得自动重装组件或重新启用 Provider。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider = 1：Codex；初始授权写入 = 1 次；随后显式 revoke = 1 次。
///   bundle CLI = `cli-v1`→`cli-v2`；受管 CLI 预期保持 `cli-v1`。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 完整安装旧版 Codex，然后从授权注册表撤销 Codex。
///   2. 将 bundle 升级为 v2，并执行 reconcile_installed。
///   3. 检查授权、受管 CLI、Provider 状态和授权写入次数。
///
/// 预期结果：
///   - 正断言：Codex 精确保持 InstallFailed，旧受管 CLI 不被自动更新。
///   - 负断言：reconcile 不得重新写授权或恢复被撤销的 Token。
#[test]
fn revoked_provider_is_not_reenabled_or_repaired_during_app_upgrade() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let home = temp.path().join("home");
    let bundle = temp.path().join("bundle");
    let paths = InstallPaths::for_home_and_bundle_bin(&home, &bundle);
    write_bundle(&paths, b"cli-v1", b"hook-v1");
    let old_installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();
    old_installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("旧版本 Codex 安装必须成功");
    registry
        .revoke(OrchestrationProvider::Codex)
        .expect("测试必须显式撤销 Codex 授权");

    write_bundle(&paths, b"cli-v2", b"hook-v2");
    let upgraded_installer = OrchestrationInstaller::new(paths.clone(), "2.0.0");
    let report = upgraded_installer.reconcile_installed(&mut registry);
    let codex = integration(&report.integrations, OrchestrationProvider::Codex);

    assert!(
        report.failures.is_empty(),
        "撤销授权的 Provider 必须被自动 reconcile 跳过，不应产生安装失败，actual={:?}",
        report.failures
    );
    assert_eq!(
        codex.aggregate_status,
        AggregateStatus::InstallFailed,
        "授权已撤销时 Codex 必须精确保持 InstallFailed"
    );
    assert!(
        matches!(
            codex.authorization_component,
            ComponentState::Failed {
                ref error_code,
                ..
            } if error_code == "authorization_revoked"
        ),
        "被撤销授权的 Codex 必须精确报告 authorization_revoked"
    );
    assert_eq!(
        fs::read(paths.managed_cli_path()).expect("旧受管 CLI 必须保留"),
        b"cli-v1",
        "没有任何有效授权 Provider 时不得自动更新共享 CLI"
    );
    assert_eq!(
        registry.authorize_calls, 1,
        "App 升级 reconcile 不得重新写入被撤销的 Codex 授权"
    );
    assert!(
        !registry.tokens.contains_key(&OrchestrationProvider::Codex),
        "被撤销的 Codex Token 不得被自动重新启用"
    );
}
