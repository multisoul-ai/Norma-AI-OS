//! InfCode hooks：Claude 式 command hooks 写入 `~/.infcode/settings.json`。
use norma::agent_hook::{
    agent_hooks_installed_for_source, hook_events_for_source, install_hook_config,
    uninstall_hook_config, HookInstallPaths, HookSource, HookTarget,
};
use std::path::Path;

fn paths_in(dir: &Path) -> HookInstallPaths {
    HookInstallPaths {
        claude_settings: dir.join("claude.json"),
        codex_hooks: dir.join("codex.json"),
        cursor_hooks: dir.join("cursor.json"),
        grok_hooks: dir.join("grok.json"),
        opencode_plugin: dir.join("opencode.js"),
        infcode_settings: dir.join(".infcode").join("settings.json"),
        pi_extension: dir.join("pi-agent/extensions/norma-agent-hook.ts"),
        kimi_config: dir.join("kimi-home").join("config.toml"),
        hermes_plugin_dir: dir.join("hermes/plugins/norma-agent-hook"),
        zcode_config: dir.join("zcode").join("config.json"),
    }
}

/// 场景：全新环境安装 InfCode hooks。
/// 数据构造：临时目录；command = `<tmp>/multisoul-agent-hook`；
/// 事件列表 = InfCode settings.json 文档事件（含 PermissionRequest / Subagent* / Turn*）。
/// 过程：install_hook_config(InfCode) → 读 settings.json → uninstall。
/// 预期：正断言每个事件各有 1 条 Norma forward --source infcode 命令，且检测为已安装；
/// 负断言：不得写入 OpenCode JS 插件路径；卸载后不得再报已安装。
#[test]
fn infcode_installs_claude_style_command_hooks_into_settings_json() {
    let temp = tempfile::tempdir().expect("应创建临时目录");
    let paths = paths_in(temp.path());
    let bin = temp.path().join("multisoul-agent-hook");
    std::fs::write(&bin, b"hook").expect("应写入 hook 二进制占位");

    install_hook_config(HookTarget::InfCode, &bin, &paths).expect("InfCode 安装应成功");
    assert!(
        paths.infcode_settings.is_file(),
        "应写入 ~/.infcode/settings.json"
    );
    assert!(
        !paths.opencode_plugin.exists(),
        "InfCode 不得走 OpenCode JS 插件路径"
    );
    assert!(
        agent_hooks_installed_for_source(HookSource::InfCode, &paths),
        "安装后应检测为已安装"
    );

    let root: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&paths.infcode_settings).unwrap())
            .expect("settings.json 应为合法 JSON");
    let events = hook_events_for_source(HookSource::InfCode);
    assert_eq!(
        events.len(),
        12,
        "InfCode 事件应为其 settings.json 文档集，不得静默退回 OpenCode 的 8 项"
    );
    assert!(
        events.contains(&"PermissionRequest") && events.contains(&"TurnStart"),
        "InfCode 事件集须含 PermissionRequest 与 TurnStart"
    );
    assert!(
        !events.contains(&"StopFailure"),
        "InfCode settings hooks 文档未列 StopFailure，不得误装"
    );
    for event in events {
        let command = root["hooks"][event][0]["hooks"][0]["command"]
            .as_str()
            .expect("应有 command");
        assert!(
            command.contains("multisoul-agent-hook")
                && command.contains("forward --source infcode"),
            "{event} 应含 Norma InfCode hook 命令，实际={command}"
        );
    }

    uninstall_hook_config(HookTarget::InfCode, &paths).expect("卸载应成功");
    assert!(
        !agent_hooks_installed_for_source(HookSource::InfCode, &paths),
        "卸载后不得再报已安装"
    );
}

/// 场景：用户已有 InfCode settings.json 时安装/卸载 Norma hooks。
/// 数据构造：settings 含自定义键 `{"keepMe":true,"hooks":{}}`。
/// 过程：install → uninstall。
/// 预期：正断言 keepMe 在安装与卸载后仍在；负断言不得整文件覆盖成仅 hooks。
#[test]
fn infcode_install_and_uninstall_preserve_user_settings_fields() {
    let temp = tempfile::tempdir().expect("应创建临时目录");
    let paths = paths_in(temp.path());
    let bin = temp.path().join("multisoul-agent-hook");
    std::fs::write(&bin, b"hook").expect("应写入 hook 二进制占位");
    std::fs::create_dir_all(paths.infcode_settings.parent().expect("父目录")).expect("建目录");
    std::fs::write(
        &paths.infcode_settings,
        "{\n  \"keepMe\": true,\n  \"hooks\": {}\n}\n",
    )
    .expect("写入用户 settings");

    install_hook_config(HookTarget::InfCode, &bin, &paths).expect("安装应成功");
    let after_install = std::fs::read_to_string(&paths.infcode_settings).expect("读取");
    assert!(
        after_install.contains("keepMe"),
        "安装后必须保留用户字段 keepMe，实际={after_install}"
    );
    assert!(
        after_install.contains("forward --source infcode"),
        "安装后应写入 Norma InfCode hook"
    );

    uninstall_hook_config(HookTarget::InfCode, &paths).expect("卸载应成功");
    let after_uninstall = std::fs::read_to_string(&paths.infcode_settings).expect("读取");
    assert!(
        after_uninstall.contains("keepMe"),
        "卸载后必须保留用户字段 keepMe，实际={after_uninstall}"
    );
    assert!(
        !after_uninstall.contains("forward --source infcode"),
        "卸载后不得残留 Norma InfCode hook"
    );
}
