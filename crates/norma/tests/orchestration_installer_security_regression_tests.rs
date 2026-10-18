use norma::agent_hook::HookInstallPaths;
use norma::orchestration::{
    AggregateStatus, AuthorizationRegistry, ComponentState, InstallPaths, OrchestrationInstaller,
    OrchestrationProvider,
};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[derive(Default)]
struct MemoryRegistry {
    tokens: HashMap<OrchestrationProvider, String>,
}

impl AuthorizationRegistry for MemoryRegistry {
    type Error = String;

    fn authorize(
        &mut self,
        provider: OrchestrationProvider,
        token: &str,
        _installed_version: &str,
    ) -> Result<(), Self::Error> {
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

fn test_paths(root: &std::path::Path) -> InstallPaths {
    InstallPaths {
        managed_root: root.join("managed"),
        bundled_cli: root.join("bundle/norma"),
        bundled_hook: root.join("bundle/multisoul-agent-hook"),
        hook_paths: HookInstallPaths {
            claude_settings: root.join(".claude/settings.json"),
            codex_hooks: root.join(".codex/hooks.json"),
            cursor_hooks: root.join(".cursor/hooks.json"),
            grok_hooks: root.join(".grok/hooks.json"),
            opencode_plugin: root.join(".config/opencode/plugins/multisoul-agent-hook.js"),
            kimi_config: (root.join(".config/opencode/plugins/multisoul-agent-hook.js"))
                .with_file_name("kimi-config.toml"),
        },
        claude_skill: root.join(".claude/skills/norma-orchestration/SKILL.md"),
        codex_skill: root.join(".codex/skills/norma-orchestration/SKILL.md"),
        cursor_rule: root.join(".cursor/rules/norma-orchestration.mdc"),
        opencode_skill: root.join(".config/opencode/skills/norma-orchestration/SKILL.md"),
    }
}

fn prepare_bundle(paths: &InstallPaths) {
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundled CLI 必须有父目录"),
    )
    .expect("测试必须创建 bundle 目录");
    fs::write(&paths.bundled_cli, b"norma-test-cli").expect("测试必须写入 bundled CLI");
    fs::write(&paths.bundled_hook, b"norma-test-hook").expect("测试必须写入 bundled Hook");
}

/// 凭证权限修复：受管 Token 被放宽到 0644 时不得继续显示 Ready。
///
/// 数据构造（含关键数值的推导过程）：
///   secure mode   = 0o600
///   insecure mode = 0o644
///   leaked bits   = 0o644 - 0o600 = group/other read 共 2 组
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 正常安装 Codex 四组件并保存原 Token。
///   2. 外部把凭证权限改成 0644，再执行 inspect。
///   3. 执行 install_or_repair，并读取权限与 Token。
///
/// 预期结果：
///   - 正断言：0644 被报告为可修复失败，repair 恢复精确 0600。
///   - 正断言：修复不轮换仍有效 Token。
///   - 负断言：权限过宽时不得显示 Ready。
#[test]
fn insecure_managed_credential_is_not_ready_and_repair_restores_0600() {
    let temp = tempfile::tempdir().expect("凭证权限测试必须创建临时目录");
    let paths = test_paths(temp.path());
    prepare_bundle(&paths);
    let installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = MemoryRegistry::default();
    installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("Codex 初始安装必须成功");
    let credential = paths.credential_path(OrchestrationProvider::Codex);
    let token_before = fs::read_to_string(&credential).expect("初始凭证必须可读取");
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o644))
        .expect("测试必须把凭证权限放宽到 0644");

    let insecure = installer.inspect(OrchestrationProvider::Codex, &registry);
    let repaired = installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("受管凭证权限应可自动修复");
    let mode_after = fs::metadata(&credential)
        .expect("修复后凭证元数据必须可读取")
        .permissions()
        .mode()
        & 0o777;
    let token_after = fs::read_to_string(&credential).expect("修复后凭证必须可读取");

    assert!(
        matches!(
            insecure.authorization_component,
            ComponentState::Failed {
                ref error_code,
                ..
            } if error_code == "credential_permissions_insecure"
        ),
        "0644 受管凭证必须报告 credential_permissions_insecure，实际：{insecure:?}"
    );
    assert_ne!(
        insecure.aggregate_status,
        AggregateStatus::Ready,
        "Provider Token 权限过宽时汇总状态不得显示 Ready"
    );
    assert_eq!(
        repaired.aggregate_status,
        AggregateStatus::Ready,
        "repair 恢复 0600 后 Provider 必须重新 Ready"
    );
    assert_eq!(
        mode_after, 0o600,
        "repair 必须把 Provider Token 权限精确恢复为 0600"
    );
    assert_eq!(
        token_after, token_before,
        "权限修复不得无故轮换仍有效的 Provider Token"
    );
}

/// OpenCode 安装器冲突：未受管同路径插件必须以 Conflict 返回并保持内容。
///
/// 数据构造（含关键数值的推导过程）：
///   plugin files = 1
///   managed markers = 0
///   install attempts = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 OpenCode 目标路径写入用户插件。
///   2. 调用 install_or_repair(OpenCode)。
///   3. 检查 Hook 组件、汇总状态与原始内容。
///
/// 预期结果：
///   - 正断言：Hook 返回精确路径 Conflict，汇总为 NeedsRepair。
///   - 负断言：不得覆盖用户插件或错误显示 Ready。
#[test]
fn opencode_installer_surfaces_unmanaged_plugin_conflict_without_overwrite() {
    let temp = tempfile::tempdir().expect("OpenCode 安装冲突测试必须创建临时目录");
    let paths = test_paths(temp.path());
    prepare_bundle(&paths);
    fs::create_dir_all(
        paths
            .hook_paths
            .opencode_plugin
            .parent()
            .expect("OpenCode 插件路径必须有父目录"),
    )
    .expect("测试必须创建 OpenCode 插件目录");
    let user_source = "export const privatePlugin = true;\n";
    fs::write(&paths.hook_paths.opencode_plugin, user_source)
        .expect("测试必须预置用户 OpenCode 插件");
    let installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = MemoryRegistry::default();

    let result = installer
        .install_or_repair(OrchestrationProvider::OpenCode, &mut registry)
        .expect("文件归属冲突应返回可展示状态，不应破坏安装事务");
    let preserved =
        fs::read_to_string(&paths.hook_paths.opencode_plugin).expect("冲突后用户插件必须仍可读取");

    assert!(
        matches!(
            result.hook_component,
            ComponentState::Conflict { ref path, .. }
                if path == &paths.hook_paths.opencode_plugin
        ),
        "OpenCode 未受管插件必须返回带精确路径的 Conflict，实际：{result:?}"
    );
    assert_eq!(
        result.aggregate_status,
        AggregateStatus::NeedsRepair,
        "OpenCode Hook 冲突必须汇总为 NeedsRepair"
    );
    assert_eq!(
        preserved, user_source,
        "安装器不得覆盖无法确认归属的 OpenCode 用户插件"
    );
    assert_ne!(
        result.aggregate_status,
        AggregateStatus::Ready,
        "OpenCode Hook 冲突未解决时不得显示 Ready"
    );
}
