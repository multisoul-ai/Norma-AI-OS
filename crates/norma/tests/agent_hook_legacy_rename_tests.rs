use norma::agent_hook::{
    agent_hooks_installed_for_source, env_var_with_legacy, has_hermes_marker, has_opencode_marker,
    has_pi_marker, install_hook_config, unmanaged_hook_path, HookInstallPaths, HookSource,
    HookTarget, LEGACY_AGENT_HOOK_BINARY_NAME, LEGACY_HERMES_PLUGIN_MARKER,
    LEGACY_OPENCODE_PLUGIN_FILE_NAME, LEGACY_PI_EXTENSION_MARKER, OPENCODE_PLUGIN_MARKER,
};
use std::fs;
use std::path::Path;

fn paths(root: &Path) -> HookInstallPaths {
    HookInstallPaths {
        claude_settings: root.join("claude-settings.json"),
        codex_hooks: root.join("codex-hooks.json"),
        cursor_hooks: root.join("cursor-hooks.json"),
        grok_hooks: root.join("grok-hooks.json"),
        opencode_plugin: root.join("norma-hook.js"),
        infcode_settings: root.join("infcode-settings.json"),
        kimi_config: root.join("kimi-config.toml"),
        pi_extension: root.join("norma-agent-hook.ts"),
        hermes_plugin_dir: root.join("hermes-plugin"),
        zcode_config: root.join("zcode-config.json"),
    }
}

/// [场景描述]：二进制改名后，存量 Claude 配置里的旧命令被识别为过期，重装后只剩新命令。
///
/// 数据构造：
///   claude-settings.json 的 SessionStart 下有一条旧名字命令 `/old/<legacy> forward --source claude`；
///   新命令路径为 `<tmp>/norma-hook`。
///
/// 执行过程：
///   1. 安装前检查 → 旧命令不匹配新名字，应判定为"未安装"；
///   2. install_hook_config(Claude) → 先移除旧命令再写入新命令；
///   3. 读回 JSON 文本检查。
///
/// 预期结果：
///   - 安装前 installed=false：否则 reconcile 永远不会升级旧配置；
///   - 安装后 installed=true；
///   - 文本含 `norma-hook`，不含旧名字：旧 hook 必须被清掉，否则同一事件重复触发。
#[test]
fn legacy_claude_hook_is_replaced_by_new_name() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = paths(temp.path());
    let legacy = format!(
        r#"{{"hooks":{{"SessionStart":[{{"hooks":[{{"command":"/old/{LEGACY_AGENT_HOOK_BINARY_NAME} forward --source claude"}}]}}]}}}}"#
    );
    fs::write(&paths.claude_settings, legacy).expect("写旧配置");
    assert!(
        !agent_hooks_installed_for_source(HookSource::Claude, &paths),
        "旧名字配置必须判为未安装，才能触发升级"
    );
    install_hook_config(HookTarget::Claude, &temp.path().join("norma-hook"), &paths)
        .expect("install 应成功");
    let text = fs::read_to_string(&paths.claude_settings).expect("读回");
    assert!(
        agent_hooks_installed_for_source(HookSource::Claude, &paths),
        "安装后应为已安装"
    );
    assert!(text.contains("norma-hook"), "应写入新名字命令");
    assert!(
        !text.contains(LEGACY_AGENT_HOOK_BINARY_NAME),
        "旧名字命令必须被清除，避免重复触发"
    );
}

/// [场景描述]：改名前的 OpenCode 旧插件文件与 Kimi 旧托管块在重装时被清理/替换。
///
/// 数据构造：
///   `<tmp>/<legacy>.js` 内容含 OPENCODE_PLUGIN_MARKER；
///   kimi-config.toml 含旧标记包裹的区块，区块外有用户行 `model = "x"`。
///
/// 执行过程：
///   1. install_hook_config(All 中的 OpenCode 与 Kimi 目标分别执行)；
///   2. 检查旧插件文件、kimi 文本。
///
/// 预期结果：
///   - 旧插件文件不存在：否则新旧插件同时加载；
///   - 新插件文件存在；
///   - kimi 文本不含旧名字、保留用户行 `model = "x"`、含新命令。
#[test]
fn legacy_opencode_plugin_and_kimi_block_are_migrated() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = paths(temp.path());
    let legacy_plugin = temp.path().join(LEGACY_OPENCODE_PLUGIN_FILE_NAME);
    fs::write(&legacy_plugin, format!("// {OPENCODE_PLUGIN_MARKER}\n")).expect("写旧插件");
    let kimi = format!(
        "model = \"x\"\n# >>> multisoul{n} managed (do not edit) >>>\n[[hooks]]\nevent = \"Stop\"\ncommand = \"/old/{LEGACY_AGENT_HOOK_BINARY_NAME} forward --source kimi\"\n# <<< multisoul{n} managed <<<\n",
        n = "-agent-hook"
    );
    fs::write(&paths.kimi_config, kimi).expect("写旧 kimi 配置");

    let command = temp.path().join("norma-hook");
    install_hook_config(HookTarget::OpenCode, &command, &paths).expect("opencode install");
    install_hook_config(HookTarget::Kimi, &command, &paths).expect("kimi install");

    assert!(!legacy_plugin.exists(), "旧 OpenCode 插件必须被删除");
    assert!(paths.opencode_plugin.exists(), "新 OpenCode 插件应存在");
    let text = fs::read_to_string(&paths.kimi_config).expect("读回 kimi");
    assert!(
        !text.contains(LEGACY_AGENT_HOOK_BINARY_NAME),
        "旧名字托管块必须被替换"
    );
    assert!(text.contains("model = \"x\""), "用户自有配置必须保留");
    assert!(text.contains("norma-hook"), "应写入新命令");
}

/// [场景描述]：改名前写下的旧标记仍被识别为 Norma 受管，新标记同样被识别，陌生内容不被误认。
///
/// 数据构造：
///   Pi 扩展文件首行写旧标记 LEGACY_PI_EXTENSION_MARKER；Hermes/OpenCode 用各自旧标记字符串；
///   另备一份 `// user plugin` 作为非受管内容。
///
/// 执行过程：
///   1. has_*_marker 分别检查旧标记、新标记、陌生内容；
///   2. unmanaged_hook_path(Pi) 检查带旧标记的扩展文件。
///
/// 预期结果：
///   - 旧标记、新标记都返回 true：否则存量用户升级后被判为"非受管"而拒绝覆盖；
///   - 陌生内容返回 false：否则会误删用户自己的插件；
///   - 带旧标记的 Pi 扩展 unmanaged_hook_path 为 None。
#[test]
fn legacy_markers_are_recognized_as_managed() {
    assert!(
        has_pi_marker(LEGACY_PI_EXTENSION_MARKER),
        "旧 Pi 标记应受管"
    );
    assert!(
        has_pi_marker("// norma-pi-extension-marker"),
        "新 Pi 标记应受管"
    );
    assert!(
        has_hermes_marker(LEGACY_HERMES_PLUGIN_MARKER),
        "旧 Hermes 标记应受管"
    );
    assert!(
        has_hermes_marker("# norma-hermes-plugin-marker"),
        "新 Hermes 标记应受管"
    );
    assert!(
        has_opencode_marker("// norma-opencode-plugin-marker"),
        "新 OpenCode 标记应受管"
    );
    assert!(!has_pi_marker("// user plugin"), "陌生内容不得当作受管");
    assert!(
        !has_opencode_marker("// user plugin"),
        "陌生内容不得当作受管"
    );

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = paths(temp.path());
    fs::write(
        &paths.pi_extension,
        format!("// {LEGACY_PI_EXTENSION_MARKER}\n"),
    )
    .expect("写旧扩展");
    assert!(
        unmanaged_hook_path(HookSource::Pi, &paths).is_none(),
        "带旧标记的 Pi 扩展不得被判为非受管"
    );
}

/// [场景描述]：`NORMA_*` 环境变量优先，缺失时回退旧前缀（改名前启动的 Agent 进程）。
///
/// 数据构造：
///   NORMA_LEGACY_ENV_PROBE_A 仅设旧前缀名；NORMA_LEGACY_ENV_PROBE_B 新旧同时设置，值不同；
///   NORMA_LEGACY_ENV_PROBE_C 都不设。
///
/// 预期结果：
///   - A 读到旧前缀的值；B 读到新前缀的值（新优先）；C 为 None。
#[test]
fn env_var_falls_back_to_legacy_prefix() {
    let legacy = |rest: &str| format!("{}{rest}", concat!("MULTISOUL", "_"));
    std::env::set_var(legacy("LEGACY_ENV_PROBE_A"), "old-a");
    std::env::set_var(legacy("LEGACY_ENV_PROBE_B"), "old-b");
    std::env::set_var("NORMA_LEGACY_ENV_PROBE_B", "new-b");
    assert_eq!(
        env_var_with_legacy("NORMA_LEGACY_ENV_PROBE_A").as_deref(),
        Some("old-a"),
        "只有旧名字时应回退读取"
    );
    assert_eq!(
        env_var_with_legacy("NORMA_LEGACY_ENV_PROBE_B").as_deref(),
        Some("new-b"),
        "新旧并存时新名字优先"
    );
    assert_eq!(
        env_var_with_legacy("NORMA_LEGACY_ENV_PROBE_C"),
        None,
        "都未设置时必须为 None"
    );
}
