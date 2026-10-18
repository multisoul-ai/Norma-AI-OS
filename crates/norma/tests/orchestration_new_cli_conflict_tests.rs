//! 新 CLI 的 hook Conflict 不授权、用户文件不被改动，以及“委派链路未验证”标记的测试。
use norma::orchestration::{
    AggregateStatus, AuthorizationRegistry, CliHomeOverrides, ComponentState, InstallPaths,
    OrchestrationInstaller, OrchestrationProvider,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

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
        Ok(self.tokens.get(&provider).is_some_and(|v| v == token))
    }
}

fn setup(root: &Path) -> OrchestrationInstaller {
    let bundle = root.join("bundle");
    fs::create_dir_all(&bundle).expect("应创建 bundle 目录");
    fs::write(bundle.join("norma"), b"norma-bin").expect("应写入 norma");
    fs::write(bundle.join("multisoul-agent-hook"), b"hook-bin").expect("应写入 hook");
    let paths = InstallPaths::for_home_bundle_bin_and_env(
        &root.join("home"),
        &bundle,
        &CliHomeOverrides::default(),
    );
    OrchestrationInstaller::new(paths, "9.9.9")
}

/// Pi / Hermes 已有非受管 hook 时：安装返回 Conflict、不授权、用户文件逐字节不变。
///
/// 数据构造：Pi 用户扩展文件内容 "// user extension"；Hermes 插件目录内 `__init__.py` 为
/// "# user plugin"（均无 Norma 标记，因此是非受管）。
/// 过程：对两者分别 install_or_repair。
/// 预期：正断言 hook 组件 Conflict、整体状态不是 Ready、文件内容与原文完全一致；
/// 负断言授权组件不是 Ready 且 registry 中没有该 provider 的令牌。
#[test]
fn unmanaged_hook_conflict_blocks_authorization_and_keeps_user_file() {
    let temp = tempfile::tempdir().expect("应创建临时目录");
    let installer = setup(temp.path());
    let hooks = installer.paths().hook_paths.clone();
    fs::create_dir_all(hooks.pi_extension.parent().expect("父目录")).expect("建目录");
    fs::write(&hooks.pi_extension, "// user extension").expect("写入");
    let hermes_file = hooks.hermes_plugin_dir.join("__init__.py");
    fs::create_dir_all(&hooks.hermes_plugin_dir).expect("建目录");
    fs::write(&hermes_file, "# user plugin").expect("写入");

    for (provider, file, original) in [
        (
            OrchestrationProvider::Pi,
            hooks.pi_extension.clone(),
            "// user extension",
        ),
        (OrchestrationProvider::Hermes, hermes_file, "# user plugin"),
    ] {
        let mut registry = MemoryRegistry::default();
        let result = installer
            .install_or_repair(provider, &mut registry)
            .unwrap_or_else(|error| panic!("{provider:?} 安装不应报错: {error}"));
        assert!(
            matches!(result.hook_component, ComponentState::Conflict { .. }),
            "{provider:?} hook 应为 Conflict: {result:?}"
        );
        assert_ne!(
            result.aggregate_status,
            AggregateStatus::Ready,
            "{provider:?} 整体状态必须可见为需处理"
        );
        assert_eq!(
            fs::read(&file).expect("读取"),
            original.as_bytes(),
            "{provider:?} 用户文件必须逐字节不变"
        );
        assert!(
            !registry.tokens.contains_key(&provider),
            "{provider:?} hook 冲突时不得授予授权"
        );
        assert!(
            !matches!(result.authorization_component, ComponentState::Ready { .. }),
            "{provider:?} 授权组件不得 Ready"
        );
    }
}

/// Kimi / ZCode 的 hook 写入共享配置文件（合并而非独占文件），不存在“同名非受管文件”冲突；
/// 用户已有配置的无关键必须保留，且安装后能正常授权。
///
/// 数据构造：Kimi config.toml 含 `# keep-me` 注释行与 `model = "x"`；ZCode config.json 含
/// `{"keepMe": true}`。
/// 预期：安装后整体 Ready；用户内容仍存在（正断言）；卸载后用户内容依旧存在。
#[test]
fn shared_config_providers_preserve_user_settings() {
    let temp = tempfile::tempdir().expect("应创建临时目录");
    let installer = setup(temp.path());
    let hooks = installer.paths().hook_paths.clone();
    fs::create_dir_all(hooks.kimi_config.parent().expect("父目录")).expect("建目录");
    fs::write(&hooks.kimi_config, "# keep-me\nmodel = \"x\"\n").expect("写入");
    fs::create_dir_all(hooks.zcode_config.parent().expect("父目录")).expect("建目录");
    fs::write(&hooks.zcode_config, "{\"keepMe\": true}").expect("写入");

    for (provider, file, needle) in [
        (
            OrchestrationProvider::Kimi,
            hooks.kimi_config.clone(),
            "keep-me",
        ),
        (
            OrchestrationProvider::ZCode,
            hooks.zcode_config.clone(),
            "keepMe",
        ),
    ] {
        let mut registry = MemoryRegistry::default();
        let result = installer
            .install_or_repair(provider, &mut registry)
            .expect("安装应成功");
        assert_eq!(
            result.aggregate_status,
            AggregateStatus::Ready,
            "{provider:?}"
        );
        assert!(
            fs::read_to_string(&file).expect("读取").contains(needle),
            "{provider:?} 安装后用户配置应保留"
        );
        installer.uninstall(provider, &mut registry).expect("卸载");
        assert!(
            fs::read_to_string(&file).expect("读取").contains(needle),
            "{provider:?} 卸载后用户配置应保留"
        );
    }
}

/// 只有四个新 provider 标注“委派链路未验证”。
///
/// 数据构造：遍历 ALL。预期：Kimi/ZCode/Pi/Hermes 为 true；Claude/Codex/Cursor/OpenCode/Grok 为 false。
#[test]
fn delegation_unverified_marks_only_new_clis() {
    use OrchestrationProvider::*;
    for provider in [Kimi, ZCode, Pi, Hermes] {
        assert!(
            provider.delegation_unverified(),
            "{provider:?} 应标注未验证"
        );
    }
    for provider in [Claude, Codex, Cursor, OpenCode, Grok] {
        assert!(
            !provider.delegation_unverified(),
            "{provider:?} 不得带未验证标注"
        );
    }
}

static HERMES_CALLS: std::sync::Mutex<Vec<bool>> = std::sync::Mutex::new(Vec::new());

fn record_hermes(enable: bool) -> bool {
    HERMES_CALLS.lock().expect("锁").push(enable);
    true
}

/// 卸载 Hermes 时只对 Norma 受管插件执行 `hermes plugins disable`。
///
/// 场景：开启 Hermes 激活（注入记录型开关）。
/// 数据构造：先在用户自有同名目录（`__init__.py` 为 "# user plugin"，无 Norma 标记）上卸载；
/// 再换一个干净 home 安装受管插件后卸载。
/// 预期：用户自有目录卸载不记录任何调用且文件保留；受管卸载恰好记录一次 disable(false)。
#[test]
fn uninstall_hermes_disables_only_managed_plugin() {
    let user_home = tempfile::tempdir().expect("应创建临时目录");
    let installer = setup(user_home.path()).with_hermes_toggle(record_hermes);
    let hooks = installer.paths().hook_paths.clone();
    let user_file = hooks.hermes_plugin_dir.join("__init__.py");
    fs::create_dir_all(&hooks.hermes_plugin_dir).expect("建目录");
    fs::write(&user_file, "# user plugin").expect("写入");
    HERMES_CALLS.lock().expect("锁").clear();
    let mut registry = MemoryRegistry::default();
    installer
        .uninstall(OrchestrationProvider::Hermes, &mut registry)
        .expect("卸载不应报错");
    assert!(
        HERMES_CALLS.lock().expect("锁").is_empty(),
        "用户自有同名插件不得被 disable"
    );
    assert_eq!(
        fs::read(&user_file).expect("读取"),
        b"# user plugin",
        "用户文件必须保留"
    );

    let managed_home = tempfile::tempdir().expect("应创建临时目录");
    let managed = setup(managed_home.path()).with_hermes_toggle(record_hermes);
    let mut registry = MemoryRegistry::default();
    managed
        .install_or_repair(OrchestrationProvider::Hermes, &mut registry)
        .expect("安装不应报错");
    HERMES_CALLS.lock().expect("锁").clear();
    managed
        .uninstall(OrchestrationProvider::Hermes, &mut registry)
        .expect("卸载不应报错");
    assert_eq!(
        *HERMES_CALLS.lock().expect("锁"),
        vec![false],
        "受管插件卸载应恰好 disable 一次"
    );
}
