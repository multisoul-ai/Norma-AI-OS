//! Configs written before question hooks existed must read as stale, so one install upgrades them.
use norma::agent_hook::{
    agent_hooks_installed_for_source, install_hook_config, HookInstallPaths, HookSource, HookTarget,
};
use serde_json::{json, Value};
use std::path::Path;

fn paths_in(dir: &Path) -> HookInstallPaths {
    HookInstallPaths {
        claude_settings: dir.join("claude.json"),
        codex_hooks: dir.join("codex.json"),
        cursor_hooks: dir.join("cursor.json"),
        grok_hooks: dir.join("grok.json"),
        opencode_plugin: dir.join("opencode.js"),
        infcode_settings: dir.join("infcode-settings.json"),
        zcode_config: dir.join("zcode").join("config.json"),
        pi_extension: dir.join("pi-agent/extensions/norma-agent-hook.ts"),
        kimi_config: dir.join("kimi-home").join("config.toml"),
        hermes_plugin_dir: dir.join("hermes/plugins/norma-agent-hook"),
    }
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn write(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

/// PreToolUse 组里带 `--request-question` 的命令数量与其 matcher。
fn question_groups(root: &Value) -> Vec<(Option<String>, u64)> {
    root["hooks"]["PreToolUse"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|group| {
            let matcher = group["matcher"].as_str().map(str::to_owned);
            group["hooks"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|h| {
                    h["command"]
                        .as_str()
                        .unwrap()
                        .ends_with("--request-question")
                })
                .map(move |h| (matcher.clone(), h["timeout"].as_u64().unwrap()))
        })
        .collect()
}

/// 场景：PR #213 之前装的 Grok / InfCode 配置只有生命周期转发，缺问答拦截项。
///
/// 数据构造：
///   1. install_hook_config 生成当前版配置
///   2. 从 PreToolUse 删掉 `--request-question` 组，模拟旧版文件（本机 11-09 12:16 的形态）
///   3. 追加一条用户自有 PreToolUse hook `echo user`
///
/// 执行过程：
///   1. agent_hooks_installed_for_source 读旧版 → 应为 false（设置页显示“安装”而不是“卸载”）
///   2. 再次 install_hook_config → 补回问答项
///   3. 第三次 install → 幂等
///
/// 预期结果：
///   - 旧版判为未安装；升级后判为已安装
///   - 问答项恰好 1 条，matcher 为 Grok=AskUserQuestion / InfCode=question，timeout 125
///   - 用户 `echo user` hook 保留
#[test]
fn stale_config_without_question_hook_is_upgraded_once() {
    for (target, source, matcher) in [
        (HookTarget::Grok, HookSource::Grok, "AskUserQuestion"),
        (HookTarget::InfCode, HookSource::InfCode, "question"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        let bin = Path::new("/opt/norma/multisoul-agent-hook");
        let file = match source {
            HookSource::Grok => paths.grok_hooks.clone(),
            _ => paths.infcode_settings.clone(),
        };
        install_hook_config(target, bin, &paths).unwrap();

        let mut stale = read(&file);
        let pre = stale["hooks"]["PreToolUse"].as_array_mut().unwrap();
        pre.retain(|group| !group.to_string().contains("--request-question"));
        pre.push(json!({"matcher": "*", "hooks": [{"type": "command", "command": "echo user"}]}));
        write(&file, &stale);
        assert!(
            question_groups(&read(&file)).is_empty(),
            "{source:?}: 旧版夹具不应含问答项，否则测试无意义"
        );

        assert!(
            !agent_hooks_installed_for_source(source, &paths),
            "{source:?}: 缺问答项的旧配置被判为已安装，设置页切换会把它卸载而不是升级"
        );

        install_hook_config(target, bin, &paths).unwrap();
        install_hook_config(target, bin, &paths).unwrap();
        let upgraded = read(&file);
        assert!(
            agent_hooks_installed_for_source(source, &paths),
            "{source:?}: 升级后仍判为未安装"
        );
        assert_eq!(
            question_groups(&upgraded),
            vec![(Some(matcher.to_owned()), 125)],
            "{source:?}: 问答项应恰好 1 条、matcher 正确、timeout 125（重复安装不得叠加）"
        );
        assert!(
            upgraded.to_string().contains("echo user"),
            "{source:?}: 升级不得删掉用户自己的 PreToolUse hook"
        );
    }
}
