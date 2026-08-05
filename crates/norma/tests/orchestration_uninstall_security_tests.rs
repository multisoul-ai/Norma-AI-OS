use norma::agent_hook::HookInstallPaths;
use norma::orchestration::{
    AuthorizationRegistry, InstallPaths, OrchestrationInstaller, OrchestrationProvider,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Default)]
struct FailingRevokeRegistry {
    tokens: HashMap<OrchestrationProvider, String>,
    fail_revoke: bool,
    revoke_attempts: usize,
}

impl AuthorizationRegistry for FailingRevokeRegistry {
    type Error = &'static str;

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
        self.revoke_attempts = self.revoke_attempts.saturating_add(1);
        if self.fail_revoke {
            return Err("forced revoke failure");
        }
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

fn install_paths(root: &Path) -> InstallPaths {
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
        },
        claude_skill: root.join(".claude/skills/norma-orchestration/SKILL.md"),
        codex_skill: root.join(".codex/skills/norma-orchestration/SKILL.md"),
        cursor_rule: root.join(".cursor/rules/norma-orchestration.mdc"),
        opencode_skill: root.join(".config/opencode/skills/norma-orchestration/SKILL.md"),
    }
}

/// 卸载授权优先级：撤销失败时不得先删除本地组件并留下仍有效的孤立 Token。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider = Codex(1)
///   受管本地组件 = Skill(1) + credential(1) + Hook config(1)
///   revoke attempts = 1；强制失败 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在隔离目录完整安装 Codex 四组件。
///   2. 将授权注册表切换为“撤销必失败”。
///   3. 请求卸载 → 必须先尝试撤销并返回错误。
///   4. 检查可重试所需的 Skill、credential 与 Hook 仍完整存在。
///
/// 预期结果：
///   - 正断言：授权撤销恰好尝试一次，卸载明确失败。
///   - 正断言：本地受管组件仍存在，用户可以原地重试。
///   - 负断言：不得出现文件已删但旧 Token 仍被授权的安全降级状态。
#[test]
fn revoke_failure_stops_uninstall_before_removing_managed_components() {
    let temp = tempfile::tempdir().expect("测试应创建隔离卸载目录");
    let paths = install_paths(temp.path());
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundle 二进制应有父目录"),
    )
    .expect("测试应创建 bundle 目录");
    fs::write(&paths.bundled_cli, b"norma").expect("测试应写入 bundled CLI");
    fs::write(&paths.bundled_hook, b"hook").expect("测试应写入 bundled Hook");
    let installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = FailingRevokeRegistry::default();
    installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("测试前置条件应完整安装 Codex");
    registry.fail_revoke = true;

    let result = installer.uninstall(OrchestrationProvider::Codex, &mut registry);

    assert!(
        result.is_err(),
        "授权撤销失败时卸载必须返回错误，不能伪装成功"
    );
    assert_eq!(
        registry.revoke_attempts, 1,
        "卸载必须且只能尝试一次 Codex 授权撤销"
    );
    assert!(
        paths.codex_skill.exists(),
        "撤销失败后 Codex Skill 必须保留以支持安全重试"
    );
    assert!(
        paths.credential_path(OrchestrationProvider::Codex).exists(),
        "撤销失败后仍有效的 Codex Token 文件不得先被删除"
    );
    assert!(
        paths.hook_paths.codex_hooks.exists(),
        "撤销失败后 Codex Hook 配置必须保留，不能形成半卸载状态"
    );
}
