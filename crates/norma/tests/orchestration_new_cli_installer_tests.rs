//! Kimi / ZCode / Pi / Hermes 接入编排安装器的参数化测试。
use norma::orchestration::{
    AggregateStatus, AuthorizationRegistry, CliHomeOverrides, ComponentState, InstallPaths,
    OrchestrationInstaller, OrchestrationProvider,
};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::Path;

const NEW_CLIS: [OrchestrationProvider; 5] = [
    OrchestrationProvider::InfCode,
    OrchestrationProvider::Kimi,
    OrchestrationProvider::ZCode,
    OrchestrationProvider::Pi,
    OrchestrationProvider::Hermes,
];

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
            .is_some_and(|value| value == token))
    }
}

fn setup(root: &Path, overrides: &CliHomeOverrides) -> OrchestrationInstaller {
    let home = root.join("home");
    let bundle = root.join("bundle");
    fs::create_dir_all(&bundle).expect("应创建 bundle 目录");
    fs::write(bundle.join("norma"), b"norma-bin").expect("应写入 norma 二进制");
    fs::write(bundle.join("multisoul-agent-hook"), b"hook-bin").expect("应写入 hook 二进制");
    let paths = InstallPaths::for_home_bundle_bin_and_env(&home, &bundle, overrides);
    OrchestrationInstaller::new(paths, "9.9.9")
}

fn no_overrides() -> CliHomeOverrides {
    CliHomeOverrides::default()
}

/// 新 CLI 进入 ALL 且各有 skill 路径与 hook 映射。
///
/// 数据构造：ALL 原有 5 项（Codex、Claude、Cursor、OpenCode、Grok），新增 InfCode + 4 项后共 10 项，
/// Grok 仍无 skill 路径，其余都必须有三个 skill 路径与 hook 映射。
/// 过程：遍历 ALL 查询 skill / presentation / ask-norma 路径。
/// 预期：正断言 ALL 含新 CLI 且长度为 10；负断言 Grok 路径为 None（未被放宽）。
#[test]
fn all_contains_new_clis_with_skill_paths_and_grok_stays_unsupported() {
    let temp = tempfile::tempdir().expect("应创建临时目录");
    let installer = setup(temp.path(), &no_overrides());
    let paths = installer.paths();

    assert_eq!(
        OrchestrationProvider::ALL.len(),
        10,
        "ALL 应为 5 个旧 + InfCode + 4 个新 provider"
    );
    for provider in NEW_CLIS {
        assert!(
            OrchestrationProvider::ALL.contains(&provider),
            "{provider:?} 应在 ALL 中"
        );
        assert!(
            paths.presentation_skill_path(provider).is_some(),
            "{provider:?} 应有展示技能路径"
        );
        assert!(
            paths.ask_norma_skill_path(provider).is_some(),
            "{provider:?} 应有 ask-norma 路径"
        );
    }
    assert!(
        paths
            .presentation_skill_path(OrchestrationProvider::Grok)
            .is_none(),
        "Grok 不得获得 skill 路径"
    );
}

/// 各 CLI skill 目录遵循已验证的默认位置与环境变量覆盖。
///
/// 数据构造：home=`<t>/home`；覆盖值 KIMI_CODE_HOME=`<t>/k`、PI_CODING_AGENT_DIR=`<t>/p`、
/// HERMES_HOME=`<t>/h`。ZCode 无环境变量覆盖。
/// 过程：先无覆盖、再带覆盖构造 InstallPaths，读取 orchestration skill 路径。
/// 预期：默认落在 ~/.kimi-code、~/.zcode、~/.pi/agent、~/.hermes 的 skills 下；覆盖后换根，
/// ZCode 不变；空字符串覆盖视同未设置。
#[test]
fn skill_dirs_follow_verified_defaults_and_env_overrides() {
    let temp = tempfile::tempdir().expect("应创建临时目录");
    let t = temp.path();
    let home = t.join("home");
    let leaf = "skills/norma-orchestration/SKILL.md";
    let default = setup(t, &no_overrides());
    let skill = |i: &OrchestrationInstaller, p| i.paths().skill_path(p).expect("应有路径");
    assert_eq!(
        skill(&default, OrchestrationProvider::InfCode),
        home.join(".infcode").join(leaf)
    );
    assert_eq!(
        skill(&default, OrchestrationProvider::Kimi),
        home.join(".kimi-code").join(leaf)
    );
    assert_eq!(
        skill(&default, OrchestrationProvider::ZCode),
        home.join(".zcode").join(leaf)
    );
    assert_eq!(
        skill(&default, OrchestrationProvider::Pi),
        home.join(".pi/agent").join(leaf)
    );
    assert_eq!(
        skill(&default, OrchestrationProvider::Hermes),
        home.join(".hermes").join(leaf)
    );

    let custom = setup(
        t,
        &CliHomeOverrides {
            kimi_code_home: Some(t.join("k").into_os_string()),
            pi_coding_agent_dir: Some(t.join("p").into_os_string()),
            hermes_home: Some(t.join("h").into_os_string()),
        },
    );
    assert_eq!(
        skill(&custom, OrchestrationProvider::Kimi),
        t.join("k").join(leaf)
    );
    assert_eq!(
        skill(&custom, OrchestrationProvider::Pi),
        t.join("p").join(leaf)
    );
    assert_eq!(
        skill(&custom, OrchestrationProvider::Hermes),
        t.join("h").join(leaf)
    );
    assert_eq!(
        skill(&custom, OrchestrationProvider::ZCode),
        home.join(".zcode").join(leaf),
        "ZCode 无目录覆盖变量，不得被其他覆盖影响"
    );

    let empty = setup(
        t,
        &CliHomeOverrides {
            kimi_code_home: Some(OsString::new()),
            ..CliHomeOverrides::default()
        },
    );
    assert_eq!(
        skill(&empty, OrchestrationProvider::Kimi),
        home.join(".kimi-code").join(leaf),
        "空覆盖值应回退默认目录"
    );
}

/// 安装修复后全部组件 Ready，卸载后只删自有文件。
///
/// 数据构造：隔离 home；每个新 CLI 单独 install_or_repair（共享二进制）。
/// 过程：install → inspect；再写入用户自有同级文件 → uninstall。
/// 预期：正断言 aggregate=Ready，skill/cli/hook/授权均 Ready，三个 skill 文件在盘；
/// 卸载后 aggregate=NotInstalled 且 skill 被移除；负断言用户自有文件仍在。
#[test]
fn install_ready_then_uninstall_removes_only_owned_files() {
    for provider in NEW_CLIS {
        let temp = tempfile::tempdir().expect("应创建临时目录");
        let installer = setup(temp.path(), &no_overrides());
        let mut registry = MemoryRegistry::default();
        let result = installer
            .install_or_repair(provider, &mut registry)
            .unwrap_or_else(|error| panic!("{provider:?} 安装应成功: {error}"));

        assert_eq!(
            result.aggregate_status,
            AggregateStatus::Ready,
            "{provider:?} 应全部 Ready: {result:?}"
        );
        for component in [
            &result.skill_component,
            &result.cli_component,
            &result.hook_component,
            &result.authorization_component,
        ] {
            assert!(
                matches!(component, ComponentState::Ready { .. }),
                "{provider:?} 组件应 Ready: {component:?}"
            );
        }
        let paths = installer.paths();
        let skills = [
            paths.skill_path(provider).expect("orchestration 路径"),
            paths
                .presentation_skill_path(provider)
                .expect("presentation 路径"),
            paths
                .ask_norma_skill_path(provider)
                .expect("ask-norma 路径"),
        ];
        for skill in &skills {
            assert!(skill.is_file(), "{provider:?} 应写入 {}", skill.display());
        }
        let bystander = skills[0].parent().expect("父目录").join("user-notes.md");
        fs::write(&bystander, "mine").expect("应写入用户文件");

        let after = installer
            .uninstall(provider, &mut registry)
            .expect("卸载应成功");
        assert_eq!(
            after.aggregate_status,
            AggregateStatus::NotInstalled,
            "{provider:?} 卸载后应 NotInstalled: {after:?}"
        );
        assert!(!skills[0].exists(), "{provider:?} 自有 skill 应被移除");
        assert!(bystander.is_file(), "{provider:?} 用户自有文件不得被删除");
    }
}

/// 用户已有同名 skill 时报 Conflict 且不覆盖。
///
/// 数据构造：先在 orchestration skill 路径写入无 Norma 标记的用户文件 "USER".
/// 预期：inspect 的 skill 组件为 Conflict；install_or_repair 后文件内容仍是 "USER"。
#[test]
fn unmanaged_skill_is_conflict_and_never_overwritten() {
    for provider in NEW_CLIS {
        let temp = tempfile::tempdir().expect("应创建临时目录");
        let installer = setup(temp.path(), &no_overrides());
        let mut registry = MemoryRegistry::default();
        let path = installer.paths().skill_path(provider).expect("路径");
        fs::create_dir_all(path.parent().expect("父目录")).expect("应建目录");
        fs::write(&path, "USER").expect("应写入用户文件");

        let before = installer.inspect(provider, &registry);
        assert!(
            matches!(before.skill_component, ComponentState::Conflict { .. }),
            "{provider:?} 应 Conflict: {before:?}"
        );
        let _ = installer.install_or_repair(provider, &mut registry);
        assert_eq!(
            fs::read_to_string(&path).expect("读取"),
            "USER",
            "{provider:?} 用户 skill 不得被覆盖"
        );
    }
}

/// 用户已有的 Pi 扩展 / Hermes 插件目录 → hook 组件报 Conflict，不被覆盖。
#[test]
fn unmanaged_hook_files_report_conflict() {
    let temp = tempfile::tempdir().expect("应创建临时目录");
    let installer = setup(temp.path(), &no_overrides());
    let registry = MemoryRegistry::default();
    let hooks = &installer.paths().hook_paths;
    fs::create_dir_all(hooks.pi_extension.parent().expect("父目录")).expect("建目录");
    fs::write(&hooks.pi_extension, "// user extension").expect("写入");
    fs::create_dir_all(&hooks.hermes_plugin_dir).expect("建目录");
    fs::write(hooks.hermes_plugin_dir.join("__init__.py"), "# user plugin").expect("写入");

    for provider in [OrchestrationProvider::Pi, OrchestrationProvider::Hermes] {
        let result = installer.inspect(provider, &registry);
        assert!(
            matches!(result.hook_component, ComponentState::Conflict { .. }),
            "{provider:?} 非自有 hook 应 Conflict: {:?}",
            result.hook_component
        );
    }
}

/// 授权隔离：每个 provider 凭据独立，Kimi 的令牌不能被 Pi 使用。
///
/// 数据构造：Kimi 与 Pi 各自安装，令牌随机；用 Kimi 令牌询问 Pi。
/// 预期：凭据文件路径互不相同；Kimi 令牌对 Pi 授权检查为 false；Pi 的凭据文件内 provider=pi。
#[test]
fn authorization_is_isolated_between_new_clis() {
    let temp = tempfile::tempdir().expect("应创建临时目录");
    let installer = setup(temp.path(), &no_overrides());
    let mut registry = MemoryRegistry::default();
    installer
        .install_or_repair(OrchestrationProvider::Kimi, &mut registry)
        .expect("Kimi 安装");
    installer
        .install_or_repair(OrchestrationProvider::Pi, &mut registry)
        .expect("Pi 安装");

    let kimi_path = installer
        .paths()
        .credential_path(OrchestrationProvider::Kimi);
    let pi_path = installer.paths().credential_path(OrchestrationProvider::Pi);
    assert_ne!(kimi_path, pi_path, "凭据文件必须按 provider 分开");
    let kimi_token = registry.tokens[&OrchestrationProvider::Kimi].clone();
    assert!(registry
        .is_authorized(OrchestrationProvider::Kimi, &kimi_token)
        .expect("检查"));
    assert!(
        !registry
            .is_authorized(OrchestrationProvider::Pi, &kimi_token)
            .expect("检查"),
        "Kimi 令牌不得授权 Pi"
    );
    assert!(
        fs::read_to_string(&pi_path)
            .expect("读取")
            .contains("\"pi\""),
        "Pi 凭据应标明 provider=pi"
    );

    // 把 Kimi 的凭据文件拷到 Pi 位置：inspect 必须判定冲突而不是继承授权。
    fs::copy(&kimi_path, &pi_path).expect("拷贝凭据");
    let forged = installer.inspect(OrchestrationProvider::Pi, &registry);
    assert!(
        matches!(
            forged.authorization_component,
            ComponentState::Conflict { .. }
        ),
        "跨 provider 凭据必须是 Conflict: {forged:?}"
    );
}

/// Hermes skill 的 frontmatter 含 version；其他 CLI 不带该行；各 skill 正文自称不借用别家。
#[test]
fn skill_frontmatter_matches_each_cli_requirements() {
    for provider in NEW_CLIS {
        let temp = tempfile::tempdir().expect("应创建临时目录");
        let installer = setup(temp.path(), &no_overrides());
        let mut registry = MemoryRegistry::default();
        installer
            .install_or_repair(provider, &mut registry)
            .expect("安装");
        let text = fs::read_to_string(installer.paths().skill_path(provider).expect("路径"))
            .expect("读取");
        let front = text.split("---").nth(1).expect("应有 frontmatter");
        assert!(
            front.contains("name: norma-orchestration"),
            "{provider:?} 需要 name"
        );
        assert!(
            front.contains("description:"),
            "{provider:?} 需要 description"
        );
        assert_eq!(
            front.contains("version: 9.9.9"),
            provider == OrchestrationProvider::Hermes,
            "{provider:?} version 行仅 Hermes 需要"
        );
        assert!(
            !text.contains("alwaysApply"),
            "{provider:?} 不得带 Cursor 专有字段"
        );
    }
}
