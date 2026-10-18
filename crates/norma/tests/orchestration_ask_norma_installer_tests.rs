use norma::agent_hook::HookInstallPaths;
use norma::orchestration::{
    AggregateStatus, AuthorizationRegistry, ComponentState, InstallPaths, OrchestrationInstaller,
    OrchestrationProvider,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Default)]
struct MemoryAuthorizationRegistry {
    tokens: HashMap<OrchestrationProvider, String>,
}

impl AuthorizationRegistry for MemoryAuthorizationRegistry {
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

fn install_paths(root: &Path) -> InstallPaths {
    InstallPaths {
        managed_root: root.join("managed"),
        bundled_cli: root.join("bundle").join("norma"),
        bundled_hook: root.join("bundle").join("multisoul-agent-hook"),
        hook_paths: HookInstallPaths {
            claude_settings: root.join(".claude/settings.json"),
            codex_hooks: root.join(".codex/hooks.json"),
            cursor_hooks: root.join(".cursor/hooks.json"),
            grok_hooks: root.join(".grok/hooks.json"),
            pi_extension: root.join("norma-agent-hook.ts"),
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

fn write_bundle(paths: &InstallPaths) {
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundle CLI 必须有父目录"),
    )
    .expect("测试必须创建 bundle 目录");
    fs::write(&paths.bundled_cli, b"ask-norma-test-cli").expect("测试必须写入 bundled CLI");
    fs::write(&paths.bundled_hook, b"ask-norma-test-hook").expect("测试必须写入 bundled Hook");
}

/// ask-norma 受管 Skill 安装契约：作为 norma-orchestration 的显式入口被安装到各 Provider，
/// description 覆盖 /ask-norma 与口语化委派，正文复用完整委派流程。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider 数 = 2（Codex 验证标准 frontmatter，Cursor 验证 alwaysApply 规则）
///   managed Skill 文件 = 3（orchestration + presentation + ask-norma），本测试只读 ask-norma
///   description 触发词 = 5（/ask-norma、问 norma、ask norma、让 norma 帮忙、给 norma 发消息）
///   委派流程关键串 = 5（workspace list、agent self、operation-id、self_dispatch_forbidden、
///   NormaServer 单例）
///   负向边界 = 1（Do not use for mere mentions）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在隔离目录分别安装 Codex 与 Cursor 的受管组件。
///   2. 读取 Codex ask-norma Skill，检查 marker、frontmatter、description 触发词与正文。
///   3. 读取 Cursor ask-norma rule，检查 alwaysApply 且不携带 name 字段。
///
/// 预期结果：
///   - 正断言：ask-norma 文件含 norma-ask-norma-managed marker 与当前版本 managed-version。
///   - 正断言：description 覆盖 /ask-norma、4 个口语委派示例与 mere mentions 负向边界。
///   - 正断言：正文含入口说明与完整委派流程（self 身份、跨 Workspace、确定性拒绝码）。
///   - 正断言：Cursor 走 alwaysApply: true 且不含 name: ask-norma。
///   - 负断言：Codex frontmatter 不得使用 alwaysApply，也不得复用 norma-orchestration 作为 name。
#[test]
fn ask_norma_skill_is_installed_as_managed_orchestration_entry_point() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let paths = install_paths(temp.path());
    write_bundle(&paths);
    let installer = OrchestrationInstaller::new(paths.clone(), "2.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();
    installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("Codex 受管组件安装应成功");
    installer
        .install_or_repair(OrchestrationProvider::Cursor, &mut registry)
        .expect("Cursor 受管组件安装应成功");

    let codex_ask_norma = paths
        .ask_norma_skill_path(OrchestrationProvider::Codex)
        .expect("Codex 应有 ask-norma Skill 路径");
    let codex_source =
        fs::read_to_string(&codex_ask_norma).expect("安装后 Codex ask-norma Skill 应可读取");
    let cursor_ask_norma = paths
        .ask_norma_skill_path(OrchestrationProvider::Cursor)
        .expect("Cursor 应有 ask-norma rule 路径");
    let cursor_source =
        fs::read_to_string(&cursor_ask_norma).expect("安装后 Cursor ask-norma rule 应可读取");
    let codex_description = codex_source
        .lines()
        .find_map(|line| line.strip_prefix("description: "))
        .expect("Codex ask-norma frontmatter 必须包含 description");

    assert!(
        codex_source.contains("# norma-ask-norma-managed")
            && codex_source.contains("# managed-version: 2.0.0"),
        "ask-norma Skill 必须携带 norma-ask-norma-managed marker 与当前版本号"
    );
    assert!(
        codex_source.contains("---\nname: ask-norma\n"),
        "Codex ask-norma 必须使用标准 name frontmatter，而非 alwaysApply 规则"
    );
    assert!(
        !codex_source.contains("alwaysApply: true"),
        "Codex ask-norma 不得使用 Cursor 专属的 alwaysApply 规则"
    );
    assert!(
        codex_description.contains("/ask-norma"),
        "description 必须把 /ask-norma 显式入口列为触发条件，实际为：{codex_description}"
    );
    assert!(
        codex_description.contains("问 norma")
            && codex_description.contains("ask norma")
            && codex_description.contains("让 norma 帮忙")
            && codex_description.contains("给 norma 发消息"),
        "description 必须覆盖 4 个口语化委派表达以提高召回，实际为：{codex_description}"
    );
    assert!(
        codex_description.contains("Do not use for mere mentions"),
        "提高召回时仍必须保留 mere mentions 负向边界，实际为：{codex_description}"
    );
    assert!(
        codex_source.contains("/ask-norma") && codex_source.contains("delegation flow"),
        "ask-norma 正文必须明确说明自己是 /ask-norma 入口并路由到委派流程"
    );
    assert!(
        codex_source.contains("agent self --format json")
            && codex_source.contains("workspace list")
            && codex_source.contains("--operation-id <operation_id>")
            && codex_source.contains("self_dispatch_forbidden")
            && codex_source.contains("Do not start one NormaServer per window"),
        "ask-norma 正文必须完整复用 orchestration 委派流程（身份、跨 Workspace、幂等、确定性拒绝、单例服务）"
    );
    assert!(
        cursor_source.contains("alwaysApply: true"),
        "Cursor ask-norma 必须使用 alwaysApply 规则而非 name frontmatter"
    );
    assert!(
        !cursor_source.contains("name: ask-norma"),
        "Cursor ask-norma 不得携带 name 字段，否则与 .mdc 规则格式冲突"
    );
    assert!(
        cursor_source.contains("# norma-ask-norma-managed"),
        "Cursor ask-norma rule 也必须携带 norma-ask-norma-managed marker"
    );

    let codex_after = installer.inspect(OrchestrationProvider::Codex, &registry);
    assert_eq!(
        codex_after.aggregate_status,
        AggregateStatus::Ready,
        "ask-norma 作为第三个受管 Skill 安装后，Codex 聚合状态必须保持 Ready"
    );
}

/// ask-norma 受管 Skill 卸载契约：uninstall 必须移除带 marker 的 ask-norma 文件，
/// 且不得误删用户拥有的同名文件。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider 数 = 1（Codex）
///   受管 ask-norma 文件 = 1（含 norma-ask-norma-managed marker）
///   用户同名冲突文件 = 1（无 marker，内容 "# user-owned ask-norma"）
///   卸载后预期保留文件 = 1（用户冲突文件）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 完整安装 Codex 受管组件，确认 ask-norma Skill 存在。
///   2. 卸载 Codex，确认受管 ask-norma 文件被移除。
///   3. 重新安装 Codex，把 ask-norma 文件替换为无 marker 的用户文件，再次卸载。
///   4. 确认用户文件被保留，未被静默删除或覆盖。
///
/// 预期结果：
///   - 正断言：卸载受管 Codex 后 ask-norma 文件不再存在。
///   - 正断言：无 marker 的用户 ask-norma 文件在卸载后被完整保留。
///   - 负断言：卸载不得删除用户拥有的文件内容。
#[test]
fn ask_norma_skill_is_removed_on_uninstall_but_preserves_user_file() {
    let temp = tempfile::tempdir().expect("测试应创建隔离卸载根目录");
    let paths = install_paths(temp.path());
    write_bundle(&paths);
    let installer = OrchestrationInstaller::new(paths.clone(), "2.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();
    installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("Codex 受管组件安装应成功");
    let ask_norma_path = paths
        .ask_norma_skill_path(OrchestrationProvider::Codex)
        .expect("Codex 应有 ask-norma Skill 路径");
    assert!(ask_norma_path.exists(), "安装后 ask-norma Skill 必须存在");

    installer
        .uninstall(OrchestrationProvider::Codex, &mut registry)
        .expect("卸载 Codex 受管组件应成功");
    assert!(
        !ask_norma_path.exists(),
        "卸载后受管 ask-norma Skill 必须被移除"
    );

    // 重新安装后用用户文件覆盖 ask-norma 路径，验证卸载不删用户文件。
    installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("重新安装 Codex 受管组件应成功");
    let user_owned = "# user-owned ask-norma\n";
    fs::write(&ask_norma_path, user_owned).expect("测试应写入用户 ask-norma 文件");
    installer
        .uninstall(OrchestrationProvider::Codex, &mut registry)
        .expect("二次卸载 Codex 受管组件应成功");
    let preserved = fs::read_to_string(&ask_norma_path).expect("用户 ask-norma 文件必须被保留");
    assert_eq!(
        preserved, user_owned,
        "卸载不得删除或覆盖无 norma-ask-norma-managed marker 的用户文件"
    );
}

/// ask-norma 受管 Skill 冲突保护契约：install_or_repair 不得覆盖无 marker 的用户 ask-norma 文件，
/// 必须精确报告 Conflict 并保持 NeedsRepair。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider 数 = 1（Codex）
///   冲突文件 = 1（无 marker 的用户 ask-norma，内容 "# user-owned ask-norma conflict"）
///   其他受管 Skill = 2（orchestration + presentation，正常安装）
///   预期 Skill 组件状态 = Conflict（精确路径）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 先写入无 marker 的用户 ask-norma 文件，再安装 Codex 受管组件。
///   2. inspect 检查 Skill 组件状态与冲突路径。
///   3. 读取 ask-norma 文件，确认用户原文被保留。
///
/// 预期结果：
///   - 正断言：Skill 组件精确报告 Conflict 且路径指向 ask-norma 文件。
///   - 正断言：聚合状态为 NeedsRepair，等待人工处理冲突。
///   - 负断言：用户 ask-norma 文件不得被覆盖为受管内容。
#[test]
fn ask_norma_user_conflict_is_reported_not_overwritten() {
    let temp = tempfile::tempdir().expect("测试应创建隔离冲突安装根目录");
    let paths = install_paths(temp.path());
    write_bundle(&paths);
    let ask_norma_path = paths
        .ask_norma_skill_path(OrchestrationProvider::Codex)
        .expect("Codex 应有 ask-norma Skill 路径");
    fs::create_dir_all(ask_norma_path.parent().expect("ask-norma Skill 应有父目录"))
        .expect("测试应创建 ask-norma Skill 目录");
    let user_owned = "# user-owned ask-norma conflict\n";
    fs::write(&ask_norma_path, user_owned).expect("测试应写入用户冲突 ask-norma 文件");

    let installer = OrchestrationInstaller::new(paths.clone(), "2.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();
    installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("存在 ask-norma 冲突时其他受管组件仍应安装成功");
    let after = installer.inspect(OrchestrationProvider::Codex, &registry);
    let preserved = fs::read_to_string(&ask_norma_path).expect("用户冲突文件必须被保留");

    assert!(
        matches!(
            after.skill_component,
            ComponentState::Conflict { ref path, .. } if path == &ask_norma_path
        ),
        "ask-norma 用户冲突必须被精确报告为带路径的 Skill Conflict"
    );
    assert_eq!(
        after.aggregate_status,
        AggregateStatus::NeedsRepair,
        "存在 ask-norma 冲突时聚合状态必须保持 NeedsRepair"
    );
    assert_eq!(
        preserved, user_owned,
        "install_or_repair 不得覆盖无 marker 的用户 ask-norma 文件"
    );
}
