use norma::agent_hook::{agent_hooks_installed_for_source, HookSource};
use norma::orchestration::{
    AggregateStatus, AuthorizationRegistry, CliHomeOverrides, ComponentState, InstallPaths,
    OrchestrationInstaller, OrchestrationProvider, ProviderIntegration,
};
use std::{collections::HashMap, fs};

#[derive(Default)]
struct Registry(HashMap<OrchestrationProvider, String>);

impl AuthorizationRegistry for Registry {
    type Error = String;

    fn authorize(
        &mut self,
        provider: OrchestrationProvider,
        token: &str,
        _: &str,
    ) -> Result<(), String> {
        self.0.insert(provider, token.into());
        Ok(())
    }

    fn revoke(&mut self, provider: OrchestrationProvider) -> Result<(), String> {
        self.0.remove(&provider);
        Ok(())
    }

    fn is_authorized(&self, provider: OrchestrationProvider, token: &str) -> Result<bool, String> {
        Ok(self.0.get(&provider).is_some_and(|stored| stored == token))
    }
}

fn installer(root: &std::path::Path) -> OrchestrationInstaller {
    installer_with_overrides(root, &CliHomeOverrides::default())
}

fn installer_with_overrides(
    root: &std::path::Path,
    overrides: &CliHomeOverrides,
) -> OrchestrationInstaller {
    let bundle = root.join("bundle");
    fs::create_dir_all(&bundle).expect("应创建隔离 bundle");
    fs::write(bundle.join("norma"), "norma-cli").expect("应写 CLI 夹具");
    fs::write(bundle.join("multisoul-agent-hook"), "norma-hook").expect("应写 Hook 夹具");
    OrchestrationInstaller::new(
        InstallPaths::for_home_bundle_bin_and_env(root, &bundle, overrides),
        "1.0.0",
    )
}

/// 场景：进程 GROK_HOME 决定实际技能和 Hook 安装位置；未设置或空值回退默认目录。
/// 数据构造：三个独立子进程，分别清除变量、设为空、设为含空格的隔离自定义根；不改父进程环境。
/// 执行过程：调用生产环境入口构建路径，安装、删除技能后修复，再卸载；自定义根同步检查独立 Hook 入口。
/// 预期：三个技能及 Hook 均位于指定根，修复/卸载沿用同一位置，自定义根时默认目录不被创建。
#[test]
fn grok_installation_honors_process_home_override() {
    if let Some(root) = std::env::var_os("NORMA_GROK_FIXTURE_ROOT") {
        let root = std::path::PathBuf::from(root);
        let home = root.join("home");
        let expected = std::path::PathBuf::from(
            std::env::var_os("NORMA_GROK_EXPECTED_ROOT").expect("子进程必须有独立的预期根"),
        );
        let fixture = installer(&home);
        let paths = InstallPaths::for_home_and_bundle_bin(
            &home,
            fixture.paths().bundled_cli.parent().expect("bundle 目录"),
        );
        assert_eq!(
            paths.hook_paths.grok_hooks,
            expected.join("hooks/multisoul-agent-hook.json"),
            "生产路径入口必须遵循 GROK_HOME，不能仅让测试显式覆盖生效"
        );
        if expected != home.join(".grok") {
            assert_eq!(
                norma::agent_hook::default_install_paths().grok_hooks,
                paths.hook_paths.grok_hooks,
                "独立 Hook 与编排安装器必须解析到同一自定义根"
            );
        }
        let installer = OrchestrationInstaller::new(paths.clone(), "1.0.0");
        let mut registry = Registry::default();
        let grok = OrchestrationProvider::Grok;
        let result = installer
            .install_or_repair(grok, &mut registry)
            .expect("应安装 Grok");
        assert_eq!(
            result.aggregate_status,
            AggregateStatus::Ready,
            "安装应就绪"
        );
        for name in ["norma-orchestration", "norma-presentation", "ask-norma"] {
            assert!(
                expected.join(format!("skills/{name}/SKILL.md")).is_file(),
                "{name} 必须落在实际配置根"
            );
        }
        assert!(
            paths.hook_paths.grok_hooks.is_file(),
            "Hook 必须落在实际配置根"
        );
        let skill = expected.join("skills/norma-presentation/SKILL.md");
        fs::remove_file(&skill).expect("模拟技能丢失");
        assert_eq!(
            installer.inspect(grok, &registry).aggregate_status,
            AggregateStatus::NeedsRepair,
            "实际配置根内技能丢失必须被发现"
        );
        installer
            .install_or_repair(grok, &mut registry)
            .expect("应修复技能");
        assert!(skill.is_file(), "修复必须写回实际配置根");
        installer
            .uninstall(grok, &mut registry)
            .expect("应从实际配置根卸载");
        assert!(!skill.exists(), "卸载必须删除实际配置根的受管技能");
        assert!(
            !agent_hooks_installed_for_source(HookSource::Grok, &paths.hook_paths),
            "卸载不得残留实际配置根的受管 Hook"
        );
        if expected != home.join(".grok") {
            assert!(!home.join(".grok").exists(), "自定义根时不得误写默认目录");
        }
        return;
    }
    for case in ["unset", "empty", "custom"] {
        let temp = tempfile::tempdir().expect("应创建独立子进程目录");
        let root = temp.path().canonicalize().expect("应解析临时目录");
        let expected = if case == "custom" {
            root.join("custom grok")
        } else {
            root.join("home/.grok")
        };
        let mut child = std::process::Command::new(std::env::current_exe().expect("测试程序路径"));
        child
            .args([
                "--exact",
                "grok_installation_honors_process_home_override",
                "--nocapture",
            ])
            .env("NORMA_GROK_FIXTURE_ROOT", &root)
            .env("NORMA_GROK_EXPECTED_ROOT", &expected)
            .env_remove("GROK_HOME");
        if case != "unset" {
            child.env(
                "GROK_HOME",
                if case == "empty" {
                    std::ffi::OsStr::new("")
                } else {
                    expected.as_os_str()
                },
            );
        }
        let output = child.output().expect("应运行隔离环境测试");
        assert!(
            output.status.success(),
            "{case} 配置根测试必须通过：{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// 场景：Grok 可安装、重复修复、卸载，技能与 Hook 落在 Grok Build 原生发现目录。
/// 数据构造：隔离 home/bundle 与内存授权注册表；第三方 Hook 和用户笔记各一份。
/// 执行过程：inspect → 连续安装两次 → 核对三个技能及 Hook → 删除一份技能并修复 → 卸载。
/// 预期：Missing → Ready → NotInstalled；令牌幂等、撤销生效，用户文件保留且 Norma Hook 消失。
#[test]
fn grok_install_repair_uninstall_uses_native_discovery_paths() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let installer = installer(temp.path());
    let paths = installer.paths();
    let mut registry = Registry::default();
    let grok = OrchestrationProvider::Grok;
    assert_eq!(
        installer.inspect(grok, &registry).aggregate_status,
        AggregateStatus::NotInstalled,
        "未安装的 Grok 必须可安装，不能被标为 Unsupported"
    );
    let hook_path = temp.path().join(".grok/hooks/multisoul-agent-hook.json");
    fs::create_dir_all(hook_path.parent().expect("Hook 父目录")).expect("应创建 Hook 目录");
    fs::write(
        &hook_path,
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"user-observer"}]}]}}"#,
    )
    .expect("应写入第三方 Hook");
    let result = installer
        .install_or_repair(grok, &mut registry)
        .expect("Grok 安装应成功");
    assert_eq!(
        result.aggregate_status,
        AggregateStatus::Ready,
        "四组件应全部就绪：{result:?}"
    );
    let token = registry.0[&grok].clone();
    installer
        .install_or_repair(grok, &mut registry)
        .expect("重复修复应成功");
    assert_eq!(registry.0[&grok], token, "重复安装不得更换已授权令牌");
    assert_eq!(
        paths.hook_paths.grok_hooks, hook_path,
        "必须使用原生 hooks/*.json 目录"
    );
    assert!(
        agent_hooks_installed_for_source(HookSource::Grok, &paths.hook_paths),
        "Grok Hook 应完整安装"
    );
    for (path, name) in [
        (paths.skill_path(grok), "norma-orchestration"),
        (paths.presentation_skill_path(grok), "norma-presentation"),
        (paths.ask_norma_skill_path(grok), "ask-norma"),
    ] {
        let expected = temp.path().join(format!(".grok/skills/{name}/SKILL.md"));
        assert_eq!(
            path.as_ref(),
            Some(&expected),
            "{name} 必须放在 Grok 原生技能目录"
        );
        let source = fs::read_to_string(&expected).expect("应写入技能");
        assert!(
            source.contains(&format!("name: {name}")),
            "{name} 必须有合法 frontmatter"
        );
        assert!(
            !source.contains("alwaysApply"),
            "Grok 技能不得混入 Cursor rule 字段"
        );
    }
    let presentation = paths.presentation_skill_path(grok).expect("展示技能路径");
    fs::remove_file(&presentation).expect("模拟受管技能丢失");
    assert_eq!(
        installer.inspect(grok, &registry).aggregate_status,
        AggregateStatus::NeedsRepair,
        "缺失技能应可修复而非就绪"
    );
    let repaired = installer.reconcile_installed(&mut registry);
    assert!(
        repaired.failures.is_empty(),
        "已授权 Grok 应自动修复：{:?}",
        repaired.failures
    );
    assert!(presentation.is_file(), "修复应恢复展示技能");
    assert_eq!(
        installer.inspect(grok, &registry).aggregate_status,
        AggregateStatus::Ready,
        "修复后四组件应恢复就绪"
    );
    assert_eq!(registry.0[&grok], token, "修复不得更换授权令牌");
    let note = hook_path.with_file_name("notes.txt");
    fs::write(&note, "user note").expect("应写用户笔记");
    let after = installer
        .uninstall(grok, &mut registry)
        .expect("应可卸载 Grok");
    assert_eq!(
        after.aggregate_status,
        AggregateStatus::NotInstalled,
        "卸载应回到可安装状态"
    );
    assert!(!registry.0.contains_key(&grok), "卸载必须撤销 Grok 授权");
    assert!(
        !paths.skill_path(grok).expect("技能路径").exists(),
        "受管技能应删除"
    );
    assert!(note.exists(), "用户文件必须保留");
    let remaining = fs::read_to_string(hook_path).expect("第三方 Hook 文件应保留");
    assert!(remaining.contains("user-observer"), "第三方 Hook 必须保留");
    assert!(
        !remaining.contains("forward --source grok"),
        "受管 Hook 必须清除"
    );
}

/// 场景：卸载另一个 Provider 时，仍已安装的 Grok 必须继续拥有共享二进制。
/// 数据构造：同一隔离 home 中安装 Claude 与 Grok，二者分别授权但共享 CLI/Hook。
/// 执行过程：安装二者 → 卸载 Claude → 重新检查 Grok → 卸载最后的 Grok。
/// 预期：Claude 卸载后 Grok 仍 Ready；最后一个 Provider 卸载后共享文件才被清除。
#[test]
fn grok_authorization_keeps_shared_binaries_when_another_provider_is_removed() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let installer = installer(temp.path());
    let mut registry = Registry::default();
    for provider in [OrchestrationProvider::Claude, OrchestrationProvider::Grok] {
        installer
            .install_or_repair(provider, &mut registry)
            .expect("应安装 Provider");
    }
    installer
        .uninstall(OrchestrationProvider::Claude, &mut registry)
        .expect("应卸载 Claude");
    assert_eq!(
        installer
            .inspect(OrchestrationProvider::Grok, &registry)
            .aggregate_status,
        AggregateStatus::Ready,
        "剩余 Grok 的共享 CLI/Hook 不得被其他 Provider 卸载"
    );
    installer
        .uninstall(OrchestrationProvider::Grok, &mut registry)
        .expect("应卸载 Grok");
    assert!(
        !installer.paths().managed_cli_path().exists(),
        "最后一个 Provider 卸载应清理受管 CLI"
    );
    assert!(
        !installer.paths().managed_hook_path().exists(),
        "最后一个 Provider 卸载应清理受管 Hook"
    );
}

/// 场景：真实 Grok Build 必须能发现安装器输出，避免仅靠自有 inspect 产生假阳性。
/// 数据构造：分别使用隔离默认根与自定义 GROK_HOME，安装三份技能与 Hook；仅在隔离配置内放开目录信任。
/// 执行过程：安装 → grok inspect --json（不启动模型、不执行 Hook）→ 检查实际发现结果。
/// 预期：三个技能路径准确，14 个原生事件均被识别，Grok 配置没有解析错误。
#[test]
#[ignore = "requires installed Grok Build; read-only discovery with isolated GROK_HOME"]
fn real_grok_discovers_installed_skills_and_hooks() {
    for custom_home in [false, true] {
        verify_real_grok_discovery(custom_home);
    }
}

fn verify_real_grok_discovery(custom_home: bool) {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp
        .path()
        .canonicalize()
        .expect("应解析 macOS 临时目录符号链接");
    let home = root.join("home");
    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace).expect("应创建独立项目目录，避免技能被作为 project 发现");
    let grok_home = if custom_home {
        root.join("custom grok")
    } else {
        home.join(".grok")
    };
    let installer = installer_with_overrides(
        &home,
        &CliHomeOverrides {
            grok_home: custom_home.then(|| grok_home.clone().into_os_string()),
            ..CliHomeOverrides::default()
        },
    );
    installer
        .install_or_repair(OrchestrationProvider::Grok, &mut Registry::default())
        .expect("应安装 Grok 编排文件");
    if custom_home {
        assert!(
            !home.join(".grok").exists(),
            "自定义配置根时安装器不得误写默认目录"
        );
    }
    fs::write(
        grok_home.join("config.toml"),
        "[folder_trust]\nenabled = false\n",
    )
    .expect("应写隔离目录信任配置");
    let output = std::process::Command::new("grok")
        .args(["inspect", "--json"])
        .env("GROK_HOME", &grok_home)
        .current_dir(workspace)
        .output()
        .expect("应运行真实 Grok inspect");
    assert!(
        output.status.success(),
        "Grok inspect 应成功：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let discovered: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("应返回配置 JSON");
    let skills = discovered["skills"].as_array().expect("应返回 skills");
    for name in ["norma-orchestration", "norma-presentation", "ask-norma"] {
        let path = grok_home.join(format!("skills/{name}/SKILL.md"));
        assert!(
            skills.iter().any(|skill| skill["name"] == name
                && skill["source"]["type"] == "user"
                && skill["source"]["path"] == path.to_string_lossy().as_ref()),
            "Grok 必须发现本次安装的 {name}，不能误用其他 CLI 的同名技能：{:?}",
            skills
                .iter()
                .filter(|skill| skill["name"] == name)
                .collect::<Vec<_>>()
        );
    }
    let hooks = discovered["hooks"].as_array().expect("应返回 hooks");
    let grok_hooks: Vec<_> = hooks
        .iter()
        .filter(|hook| {
            hook["target"]
                .as_str()
                .is_some_and(|target| target.contains("forward --source grok"))
        })
        .collect();
    assert_eq!(
        grok_hooks.len(),
        14,
        "Grok 必须识别 14 个已安装事件：{grok_hooks:?}"
    );
    for event in [
        "session_start",
        "user_prompt_submit",
        "pre_tool_use",
        "stop",
        "session_end",
    ] {
        assert!(
            grok_hooks.iter().any(|hook| hook["event"] == event),
            "关键事件 {event} 必须可发现"
        );
    }
    let errors = discovered["hookLoadErrors"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !errors.iter().any(|error| error
            .as_str()
            .is_some_and(|text| text.contains(grok_home.to_string_lossy().as_ref()))),
        "原生 Grok Hook 不得出现加载错误：{errors:?}"
    );
    println!(
        "Grok discovery: custom_home={custom_home}, native_skills=3, hooks={}",
        grok_hooks.len()
    );
}

/// Provider 汇总状态：只有四组件全部 Ready 才能显示 Ready，Grok 未安装时支持安装。
///
/// 数据构造（含关键数值的推导过程）：
///   组件数 = 4：Skill、CLI、Hook、授权
///   Codex：3×Ready + Hook=Missing → 非 Ready
///   Claude：4×Ready → Ready
///   Grok：4×Missing → NotInstalled
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别构造三张 ProviderIntegration。
///   2. 由组件状态纯函数计算 aggregate_status。
///   3. 比较缺 Hook、全就绪和未安装三种结果。
///
/// 预期结果：
///   - 正断言：Claude=Ready，Codex=NeedsRepair，Grok=NotInstalled。
///   - 负断言：Codex 缺 Hook 时不得错误显示 Ready。
#[test]
fn aggregate_status_requires_all_four_components_including_grok() {
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
    let grok = ProviderIntegration::from_components(
        OrchestrationProvider::Grok,
        ComponentState::Missing,
        ComponentState::Missing,
        ComponentState::Missing,
        ComponentState::Missing,
    );

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
        AggregateStatus::NotInstalled,
        "Grok 未安装时应支持安装"
    );
    assert_ne!(
        codex.aggregate_status,
        AggregateStatus::Ready,
        "缺失任一组件的 Provider 不得显示 Ready"
    );
}
