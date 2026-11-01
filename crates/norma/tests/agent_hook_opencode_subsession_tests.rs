use norma::agent_hook::{install_hook_config, HookInstallPaths, HookTarget};
use std::path::PathBuf;
use std::process::Command;

/// 在 node 里加载真实安装出的插件，喂 OpenCode 事件，收集它 POST 给 Norma 的 provider_event。
const DRIVER: &str = r#"
import http from "node:http";
const posted = [];
const server = http.createServer((req, res) => {
  let body = "";
  req.on("data", (chunk) => (body += chunk));
  req.on("end", () => {
    const event = JSON.parse(body);
    posted.push(`${event.provider_event}:${event.session_id}`);
    res.end("ok");
  });
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
process.env.MULTISOUL_NODE_ID = "5";
process.env.MULTISOUL_HOOK_TOKEN = "t";
process.env.MULTISOUL_HOOK_ENDPOINT = `http://127.0.0.1:${server.address().port}/hook`;
const plugin = await (await import(process.argv[2])).default();
const fire = (type, properties) => plugin.event({ event: { type, properties } });
await fire("session.created", { info: { id: "root", directory: "/w" } });
await fire("session.created", { info: { id: "child", parentID: "root", directory: "/w" } });
await plugin["tool.execute.before"]({ tool: "read", sessionID: "child" }, { args: {} });
await fire("session.status", { sessionID: "child", status: { type: "idle" } });
await fire("session.deleted", { info: { id: "child", parentID: "root" } });
await fire("session.status", { sessionID: "root", status: { type: "idle" } });
server.close();
console.log(JSON.stringify(posted));
"#;

/// OpenCode 子 Agent 的子会话不得把父卡片打成 Ready / Done / Ended。
///
/// 数据构造：root 会话；child 会话 parentID=root。child 里跑一个工具，然后 idle、删除；最后 root idle。
///
/// 执行过程：安装真实插件 → 复制成 .mjs → node 驱动事件 → 读取 POST 序列。
///
/// 预期结果：
///   - 正断言：序列恰为 SessionStart:root、PreToolUse:child、Stop:root。
///   - 负断言：没有 SessionStart:child、Stop:child、SessionEnd:child。
#[test]
fn opencode_child_session_lifecycle_does_not_reach_parent_card() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("shortcut: 本机没有 node，跳过 OpenCode 插件行为测试");
        return;
    }
    let temp = tempfile::tempdir().expect("应能创建临时目录");
    let plugin_path = temp.path().join("multisoul-agent-hook.js");
    let paths = HookInstallPaths {
        claude_settings: temp.path().join("claude-settings.json"),
        codex_hooks: temp.path().join("codex-hooks.json"),
        cursor_hooks: temp.path().join("cursor-hooks.json"),
        grok_hooks: temp.path().join("grok-hooks.json"),
        pi_extension: temp.path().join("norma-agent-hook.ts"),
        opencode_plugin: plugin_path.clone(),
        infcode_settings: temp.path().join("infcode-settings.json"),
        kimi_config: temp.path().join("kimi-config.toml"),
        hermes_plugin_dir: PathBuf::from("/nonexistent-hermes-plugin-dir"),
        zcode_config: temp.path().join("zcode-config.json"),
    };
    install_hook_config(
        HookTarget::OpenCode,
        &temp.path().join("multisoul-agent-hook"),
        &paths,
    )
    .expect("OpenCode 插件应能安装");
    let module = temp.path().join("plugin.mjs");
    std::fs::copy(&plugin_path, &module).expect("应能复制插件");
    let driver = temp.path().join("driver.mjs");
    std::fs::write(&driver, DRIVER).expect("应能写入驱动脚本");

    let output = Command::new("node")
        .arg(&driver)
        .arg(format!("file://{}", module.display()))
        .output()
        .expect("node 应能运行");
    assert!(
        output.status.success(),
        "驱动脚本失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let posted: Vec<String> =
        serde_json::from_slice(&output.stdout).expect("驱动脚本应输出 JSON 数组");
    assert_eq!(
        posted,
        vec!["SessionStart:root", "PreToolUse:child", "Stop:root"],
        "子会话只应透出工具活动，生命周期事件不得到达父卡片"
    );
}

/// 旧版 Norma 插件（有 marker、内容过期）不算已安装，重新安装会换成当前版本。
///
/// 数据构造：插件文件只含 OPENCODE_PLUGIN_MARKER 和一行旧代码。
///
/// 执行过程：检查安装状态 → install_hook_config → 再检查。
///
/// 预期结果：
///   - 正断言：安装后状态为 true，内容不再是旧代码。
///   - 负断言：过期插件安装前不得报告为已安装（否则启动 reconcile 不会刷新它）。
#[test]
fn stale_managed_opencode_plugin_is_refreshed() {
    use norma::agent_hook::{agent_hooks_installed_at, HookSource, OPENCODE_PLUGIN_MARKER};
    let temp = tempfile::tempdir().expect("应能创建临时目录");
    let plugin_path = temp.path().join("multisoul-agent-hook.js");
    let stale = format!("{OPENCODE_PLUGIN_MARKER}\nexport default async () => ({{}});\n");
    std::fs::write(&plugin_path, &stale).expect("应能写入旧插件");
    assert!(
        !agent_hooks_installed_at(&plugin_path, HookSource::OpenCode),
        "过期的受管插件不得报告为已安装"
    );
    let mut paths = norma::agent_hook::default_install_paths();
    paths.opencode_plugin = plugin_path.clone();
    install_hook_config(HookTarget::OpenCode, &temp.path().join("hook"), &paths)
        .expect("受管插件应可覆盖");
    assert!(agent_hooks_installed_at(&plugin_path, HookSource::OpenCode));
    assert_ne!(std::fs::read_to_string(&plugin_path).unwrap(), stale);
}
