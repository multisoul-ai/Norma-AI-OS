use norma::agent_hook::HookInstallPaths;
use norma::orchestration::{
    load_provider_authorization, AggregateStatus, AuthorizationRegistry, ComponentState,
    InstallPaths, OrchestrationInstaller, OrchestrationProvider, ProviderIntegration,
};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[derive(Default)]
struct MemoryAuthorizationRegistry {
    tokens: HashMap<OrchestrationProvider, String>,
    revoked: Vec<OrchestrationProvider>,
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
        self.revoked.push(provider);
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
            claude_settings: root.join(".claude").join("settings.json"),
            codex_hooks: root.join(".codex").join("hooks.json"),
            cursor_hooks: root.join(".cursor").join("hooks.json"),
            grok_hooks: root.join(".grok").join("hooks.json"),
            pi_extension: root.join("norma-agent-hook.ts"),
            opencode_plugin: root
                .join(".config")
                .join("opencode")
                .join("plugins")
                .join("multisoul-agent-hook.js"),
        },
        claude_skill: root
            .join(".claude")
            .join("skills")
            .join("norma-orchestration")
            .join("SKILL.md"),
        codex_skill: root
            .join(".codex")
            .join("skills")
            .join("norma-orchestration")
            .join("SKILL.md"),
        cursor_rule: root
            .join(".cursor")
            .join("rules")
            .join("norma-orchestration.mdc"),
        opencode_skill: root
            .join(".config")
            .join("opencode")
            .join("skills")
            .join("norma-orchestration")
            .join("SKILL.md"),
    }
}

/// 默认安装路径：Provider 发现路径、共享二进制和凭据必须从同一用户根稳定推导。
///
/// 数据构造（含关键数值的推导过程）：
///   home = `<temp>/home`
///   bundle bin = `<temp>/bundle`
///   共享二进制数 = 2：norma + multisoul-agent-hook
///   Provider 能力路径数 = 4：Claude、Codex、Cursor、OpenCode
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用显式 home 与 bundle bin 构造 InstallPaths。
///   2. 分别读取受管根、二进制、Skill/rule 和凭据路径。
///   3. 验证路径只落在用户目录与 App bundle，不写系统级目录。
///
/// 预期结果：
///   - 正断言：共享二进制落在用户 Application Support，bundle 来源位于传入目录。
///   - 正断言：四个 Provider 使用各自原生可发现路径。
///   - 负断言：任何安装目标都不得落到 `/usr/local`。
#[test]
fn default_install_paths_are_user_scoped_and_provider_native() {
    let temp = tempfile::tempdir().expect("测试应创建隔离路径根目录");
    let home = temp.path().join("home");
    let bundle_bin = temp.path().join("bundle");
    let paths = InstallPaths::for_home_and_bundle_bin(&home, &bundle_bin);

    assert_eq!(
        paths.managed_cli_path(),
        home.join("Library/Application Support/Norma OS/bin/norma"),
        "共享 norma CLI 必须安装到用户级稳定路径"
    );
    assert_eq!(
        paths.bundled_hook,
        bundle_bin.join("multisoul-agent-hook"),
        "Hook 安装源必须指向 App bundle 的同级二进制"
    );
    assert_eq!(
        paths.claude_skill,
        home.join(".claude/skills/norma-orchestration/SKILL.md"),
        "Claude 必须使用原生 Skill 发现目录"
    );
    assert_eq!(
        paths.codex_skill,
        home.join(".codex/skills/norma-orchestration/SKILL.md"),
        "Codex 必须使用原生 Skill 发现目录"
    );
    assert_eq!(
        paths.presentation_skill_path(OrchestrationProvider::Codex),
        Some(home.join(".codex/skills/norma-presentation/SKILL.md")),
        "Codex 展示能力必须安装为独立 norma-presentation Skill"
    );
    assert_eq!(
        paths.cursor_rule,
        home.join(".cursor/rules/norma-orchestration.mdc"),
        "Cursor 必须使用原生 rule 发现目录"
    );
    assert_eq!(
        paths.opencode_skill,
        home.join(".config/opencode/skills/norma-orchestration/SKILL.md"),
        "OpenCode 必须使用原生 Skill 发现目录"
    );
    assert!(
        !paths
            .managed_cli_path()
            .starts_with(Path::new("/usr/local")),
        "安装器不得要求 sudo 或写入系统级 /usr/local"
    );
}

/// Provider 汇总状态：只有四组件全部 Ready 才能显示 Ready，Grok 始终 Unsupported。
///
/// 数据构造（含关键数值的推导过程）：
///   组件数 = 4：Skill、CLI、Hook、授权
///   Codex：3×Ready + Hook=Missing → 非 Ready
///   Claude：4×Ready → Ready
///   Grok：4×Unsupported → Unsupported
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别构造三张 ProviderIntegration。
///   2. 由组件状态纯函数计算 aggregate_status。
///   3. 比较缺 Hook、全就绪和不支持三种结果。
///
/// 预期结果：
///   - 正断言：Claude=Ready，Codex=NeedsRepair，Grok=Unsupported。
///   - 负断言：Codex 缺 Hook 时不得错误显示 Ready。
#[test]
fn aggregate_status_requires_all_four_components_and_keeps_grok_unsupported() {
    let ready = ComponentState::Ready {
        version: "1.0.0".to_string(),
    };
    let codex = ProviderIntegration::from_components(
        OrchestrationProvider::Codex,
        ready.clone(),
        ready.clone(),
        ComponentState::Missing,
        ready.clone(),
    );
    let claude = ProviderIntegration::from_components(
        OrchestrationProvider::Claude,
        ready.clone(),
        ready.clone(),
        ready.clone(),
        ready,
    );
    let grok = ProviderIntegration::unsupported(OrchestrationProvider::Grok);

    assert_eq!(
        codex.aggregate_status,
        AggregateStatus::NeedsRepair,
        "Hook 缺失时 Codex 汇总状态必须是 NeedsRepair"
    );
    assert_eq!(
        claude.aggregate_status,
        AggregateStatus::Ready,
        "四组件全部 Ready 时 Claude 才能汇总为 Ready"
    );
    assert_eq!(
        grok.aggregate_status,
        AggregateStatus::Unsupported,
        "Grok 首版必须始终汇总为 Unsupported"
    );
    assert_ne!(
        codex.aggregate_status,
        AggregateStatus::Ready,
        "缺失任一组件的 Provider 不得显示 Ready"
    );
}

/// Codex 幂等安装：写入受管 Skill、共享 CLI、Hook 与 0600 独立凭据，重复安装不换 Token。
///
/// 数据构造（含关键数值的推导过程）：
///   bundled CLI bytes = b"#!/bin/sh\necho norma\n"（21 bytes）
///   required_version = "1.0.0"
///   Provider 数 = 1（Codex），组件数 = 4
///   凭据权限 = 0o600；重复安装次数 = 2；预期 authorize 调用数 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在临时 bundle 写入可执行 norma，创建空授权注册表。
///   2. 第一次 install_or_repair(Codex) 安装四组件并登记 Token 哈希边界。
///   3. 读取 Skill、CLI、Hook、凭据及权限，再次执行相同安装。
///   4. 比较两次凭据 Token 与受管文件内容。
///
/// 预期结果：
///   - 正断言：两次结果均 Ready，CLI 字节与 bundle 一致，Hook 可检测。
///   - 正断言：Token 文件权限为 0600，重复安装保留同一 Token。
///   - 负断言：第二次安装不得重复写授权；Skill/Hook/JSON 状态中不得出现 Token 明文。
#[test]
fn codex_install_is_idempotent_and_keeps_token_out_of_skill_and_status() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let paths = install_paths(temp.path());
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundled CLI 应有父目录"),
    )
    .expect("测试应创建 bundle 目录");
    let cli_bytes = b"#!/bin/sh\necho norma\n";
    fs::write(&paths.bundled_cli, cli_bytes).expect("测试应写入 bundled CLI");
    fs::write(&paths.bundled_hook, b"#!/bin/sh\nexit 0\n").expect("测试应写入 bundled Hook");
    fs::set_permissions(&paths.bundled_cli, fs::Permissions::from_mode(0o755))
        .expect("测试 bundled CLI 应标记为可执行");
    let installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();

    let first = installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("Codex 首次安装四组件应成功");
    let credential_path = paths.credential_path(OrchestrationProvider::Codex);
    let first_credential = fs::read_to_string(&credential_path).expect("Codex 凭据文件应已创建");
    let first_skill = fs::read_to_string(&paths.codex_skill).expect("Codex Skill 应已安装");
    let first_hook =
        fs::read_to_string(&paths.hook_paths.codex_hooks).expect("Codex Hook 应已安装");
    let second = installer
        .install_or_repair(OrchestrationProvider::Codex, &mut registry)
        .expect("Codex 重复安装应幂等成功");
    let second_credential = fs::read_to_string(&credential_path).expect("重复安装后凭据仍应存在");
    let credential_mode = fs::metadata(&credential_path)
        .expect("凭据元数据应可读取")
        .permissions()
        .mode()
        & 0o777;
    let authorization = load_provider_authorization(&paths, OrchestrationProvider::Codex)
        .expect("CLI 应能读取受管 Codex 授权")
        .expect("已安装 Codex 必须存在授权材料");

    assert_eq!(
        first.aggregate_status,
        AggregateStatus::Ready,
        "Codex 四组件首次安装完成后应重新检查为 Ready"
    );
    assert_eq!(
        second.aggregate_status,
        AggregateStatus::Ready,
        "Codex 重复安装后仍应保持 Ready"
    );
    assert_eq!(
        fs::read(paths.managed_cli_path()).expect("共享 CLI 应可读取"),
        cli_bytes,
        "受管共享 CLI 字节必须与 App bundle 中的 norma 一致"
    );
    assert_eq!(
        credential_mode, 0o600,
        "Provider Token 配置文件权限必须精确为 0600"
    );
    assert_eq!(
        authorization.provider,
        norma::protocol::AgentProfile::Codex,
        "读取的授权必须保留 Codex Provider 身份"
    );
    assert_eq!(
        Some(authorization.token.as_str()),
        registry
            .tokens
            .get(&OrchestrationProvider::Codex)
            .map(String::as_str),
        "CLI 请求使用的 Token 必须与安装器登记的 Codex Token 完全一致"
    );
    assert_eq!(
        second_credential, first_credential,
        "幂等重复安装不得无故轮换仍然有效的长期 Token"
    );
    assert_eq!(
        registry.authorize_calls, 1,
        "四组件已 Ready 时重复安装不得再次写入 Provider 授权记录"
    );
    let token = registry
        .tokens
        .get(&OrchestrationProvider::Codex)
        .expect("授权注册表应保存 Codex Token 边界");
    assert!(
        !first_skill.contains(token),
        "Token 不得写入供应商中立的编排 Skill"
    );
    assert!(
        !first_hook.contains(token),
        "Token 不得写入普通 Agent Hook 配置"
    );
    assert!(
        !format!("{first:?}{second:?}").contains(token),
        "ProviderIntegration UI/调试状态不得暴露 Token"
    );
}

/// 用户文件冲突：未带 Norma 受管标记的目标 Skill 不得被安装或修复覆盖。
///
/// 数据构造（含关键数值的推导过程）：
///   目标 = Cursor 原生 rule 路径
///   用户内容 = "# My private Cursor rule"（无 Norma marker）
///   bundled CLI = 合法可执行测试文件
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 预先在 Cursor rule 路径写入用户文件。
///   2. 执行 install_or_repair(Cursor)。
///   3. 重新读取冲突文件并检查组件/汇总状态。
///
/// 预期结果：
///   - 正断言：Skill 组件为 Conflict，汇总状态为 NeedsRepair。
///   - 正断言：冲突路径与原因可见，原用户内容逐字保留。
///   - 负断言：不得把用户文件替换成 Norma 模板或错误显示 Ready。
#[test]
fn installer_reports_conflict_without_overwriting_unmanaged_user_skill() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let paths = install_paths(temp.path());
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundled CLI 应有父目录"),
    )
    .expect("测试应创建 bundle 目录");
    fs::write(&paths.bundled_cli, b"cursor-test-cli").expect("测试应写入 bundled CLI");
    fs::write(&paths.bundled_hook, b"cursor-test-hook").expect("测试应写入 bundled Hook");
    fs::create_dir_all(paths.cursor_rule.parent().expect("Cursor rule 应有父目录"))
        .expect("测试应创建 Cursor rule 目录");
    let user_content = "# My private Cursor rule\n";
    fs::write(&paths.cursor_rule, user_content).expect("测试应预置用户 Cursor rule");
    let installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();

    let result = installer
        .install_or_repair(OrchestrationProvider::Cursor, &mut registry)
        .expect("组件冲突应作为真实状态返回，不应使整个检查崩溃");
    let preserved = fs::read_to_string(&paths.cursor_rule).expect("冲突后的用户文件仍应存在");

    assert!(
        matches!(
            result.skill_component,
            ComponentState::Conflict { ref path, .. } if path == &paths.cursor_rule
        ),
        "Cursor 用户 rule 冲突必须返回包含精确路径的 Conflict"
    );
    assert_eq!(
        result.aggregate_status,
        AggregateStatus::NeedsRepair,
        "文件冲突时 Provider 汇总状态必须是 NeedsRepair"
    );
    assert_eq!(
        preserved, user_content,
        "安装器不得覆盖无法确认归属的用户 Cursor rule"
    );
    assert_ne!(
        result.aggregate_status,
        AggregateStatus::Ready,
        "存在未解决文件冲突时不得显示 Ready"
    );
    assert!(
        !preserved.contains("norma-orchestration-managed"),
        "用户文件不得被静默替换成 Norma 受管模板"
    );
}

/// 精确卸载：只删除目标 Provider 的受管内容，并撤销授权，其他用户 Hook 保留。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider = Claude
///   用户 Hook = command "echo keep-me"
///   Norma Hook + Skill + Credential = 安装器创建
///   审计替身 = registry.revoked（只记录撤销，不删除历史）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 预置一个第三方 Claude Hook，再安装 Claude 编排组件。
///   2. 执行 uninstall(Claude)。
///   3. 检查 Skill/凭据删除、授权撤销和第三方 Hook 保留。
///
/// 预期结果：
///   - 正断言：Claude 受管组件进入 Missing，registry 只撤销 Claude。
///   - 正断言：第三方 Hook 内容仍存在。
///   - 负断言：不得删除整个 settings.json 或把 Grok 标记为已撤销。
#[test]
fn uninstall_removes_only_managed_provider_content_and_preserves_user_hook() {
    let temp = tempfile::tempdir().expect("测试应创建隔离安装根目录");
    let paths = install_paths(temp.path());
    fs::create_dir_all(
        paths
            .bundled_cli
            .parent()
            .expect("测试 bundled CLI 应有父目录"),
    )
    .expect("测试应创建 bundle 目录");
    fs::write(&paths.bundled_cli, b"claude-test-cli").expect("测试应写入 bundled CLI");
    fs::write(&paths.bundled_hook, b"claude-test-hook").expect("测试应写入 bundled Hook");
    fs::create_dir_all(
        paths
            .hook_paths
            .claude_settings
            .parent()
            .expect("Claude settings 应有父目录"),
    )
    .expect("测试应创建 Claude settings 目录");
    fs::write(
        &paths.hook_paths.claude_settings,
        r#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"echo keep-me"}]}]}}"#,
    )
    .expect("测试应预置第三方 Claude Hook");
    let installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
    let mut registry = MemoryAuthorizationRegistry::default();
    installer
        .install_or_repair(OrchestrationProvider::Claude, &mut registry)
        .expect("Claude 编排安装应成功");

    let result = installer
        .uninstall(OrchestrationProvider::Claude, &mut registry)
        .expect("Claude 精确卸载应成功");
    let settings = fs::read_to_string(&paths.hook_paths.claude_settings)
        .expect("卸载后包含第三方 Hook 的 settings.json 必须保留");

    assert_eq!(
        result.aggregate_status,
        AggregateStatus::NotInstalled,
        "卸载目标 Provider 后四个受管组件应汇总为 NotInstalled"
    );
    assert!(
        !paths.claude_skill.exists(),
        "卸载必须删除 Claude 的受管 Skill"
    );
    assert!(
        !paths
            .presentation_skill_path(OrchestrationProvider::Claude)
            .expect("Claude 应有 presentation Skill 路径")
            .exists(),
        "卸载必须同时删除 Claude 的受管展示 Skill"
    );
    assert!(
        !paths
            .credential_path(OrchestrationProvider::Claude)
            .exists(),
        "卸载必须删除 Claude Provider Token 配置"
    );
    assert_eq!(
        registry.revoked,
        vec![OrchestrationProvider::Claude],
        "卸载必须只撤销目标 Claude Provider 的授权"
    );
    assert!(
        settings.contains("echo keep-me"),
        "卸载必须保留用户原有的第三方 Claude Hook"
    );
    assert!(
        !settings.contains("multisoul-agent-hook"),
        "卸载后 Claude 配置不得残留 Norma 受管 Hook"
    );
    assert!(
        !registry.revoked.contains(&OrchestrationProvider::Grok),
        "卸载 Claude 不得错误撤销不支持的 Grok"
    );
}
