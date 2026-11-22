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
process.env.NORMA_NODE_ID = "5";
process.env.NORMA_HOOK_TOKEN = "t";
process.env.NORMA_HOOK_ENDPOINT = `http://127.0.0.1:${server.address().port}/hook`;
const plugin = (await import(process.argv[2])).default;
if (typeof plugin === "function" || typeof plugin?.setup !== "function" || typeof plugin?.id !== "string") {
  console.error("OpenCode 2 plugin export must be { id, setup }");
  process.exit(2);
}
const queue = [];
let notify = () => {};
const controller = new AbortController();
const iterable = {
  async *[Symbol.asyncIterator]() {
    while (!controller.signal.aborted) {
      if (queue.length === 0) {
        await new Promise((resolve) => {
          notify = resolve;
          if (controller.signal.aborted) resolve();
        });
        continue;
      }
      yield queue.shift();
    }
  },
};
await plugin.setup({
  event: { subscribe() { return iterable; } },
  session: { async hook() {} },
});
const push = (event) => {
  queue.push(event);
  const wake = notify;
  notify = () => {};
  wake();
};
push({ type: "session.created", data: { sessionID: "root", location: { directory: "/w" } } });
push({ type: "session.created", data: { sessionID: "child", parentID: "root" } });
push({ type: "session.execution.started", data: { sessionID: "root" } });
push({ type: "session.tool.input.started", data: { sessionID: "child", name: "read", id: "call-1" } });
push({ type: "session.status", data: { sessionID: "child", status: { type: "idle" } } });
push({ type: "session.deleted", data: { sessionID: "child" } });
push({ type: "session.status", data: { sessionID: "root", status: { type: "idle" } } });
const deadline = Date.now() + 2000;
while (posted.length < 4 && Date.now() < deadline) {
  await new Promise((resolve) => setTimeout(resolve, 20));
}
controller.abort();
notify();
server.close();
console.log(JSON.stringify(posted));
"#;

/// OpenCode 子 Agent 的子会话不得把父卡片打成 Ready / Done / Ended。
///
/// 数据构造：OpenCode 2 事件包在 data 里，不再用 properties。
///   root 无 parentID；child.parentID=root。root 先 execution.started，child 再 tool.input.started。
///   child idle 与 deleted 之后，root 才 idle。
///
/// 执行过程：安装真实插件 → 以 { id, setup } 调用 setup → 订阅流推入上述 7 个事件 → 读取 POST 序列。
///
/// 预期结果：
///   - 正断言：序列恰为 SessionStart:root、UserPromptSubmit:root、PreToolUse:child、Stop:root。
///   - 负断言：没有 SessionStart:child、UserPromptSubmit:child、Stop:child、SessionEnd:child。
#[test]
fn opencode_child_session_lifecycle_does_not_reach_parent_card() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("shortcut: 本机没有 node，跳过 OpenCode 插件行为测试");
        return;
    }
    let temp = tempfile::tempdir().expect("应能创建临时目录");
    let plugin_path = temp.path().join("norma-hook.js");
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
        &temp.path().join("norma-hook"),
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
        vec![
            "SessionStart:root",
            "UserPromptSubmit:root",
            "PreToolUse:child",
            "Stop:root"
        ],
        "父会话应进入运行中，子会话只透出工具活动，其开始/空闲/结束不得改父卡片"
    );
}

const FORM_DRIVER: &str = r#"
import http from "node:http";
const posted = [];
const server = http.createServer((req, res) => {
  let body = "";
  req.on("data", (chunk) => (body += chunk));
  req.on("end", () => {
    posted.push(JSON.parse(body).provider_event + ":" + JSON.parse(body).session_id);
    res.end("ok");
  });
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
process.env.NORMA_NODE_ID = "5";
process.env.NORMA_HOOK_TOKEN = "t";
process.env.NORMA_HOOK_ENDPOINT = `http://127.0.0.1:${server.address().port}/hook`;
const plugin = (await import(process.argv[2])).default;
const queue = [];
let notify = () => {};
const controller = new AbortController();
const iterable = {
  async *[Symbol.asyncIterator]() {
    while (!controller.signal.aborted) {
      if (queue.length === 0) {
        await new Promise((resolve) => {
          notify = resolve;
          if (controller.signal.aborted) resolve();
        });
        continue;
      }
      yield queue.shift();
    }
  },
};
await plugin.setup({ event: { subscribe() { return iterable; } }, session: { async hook() {} } });
const push = (event) => {
  queue.push(event);
  const wake = notify;
  notify = () => {};
  wake();
};
push({ type: "session.created", data: { sessionID: "child", parentID: "root" } });
push({ type: "form.created", data: { form: { id: "frm_root", sessionID: "root", title: "Color" } } });
push({ type: "form.created", data: { form: { id: "frm_child", sessionID: "child", title: "Child" } } });
push({ type: "form.replied", data: { id: "frm_root", sessionID: "root", answer: { color: "Green" } } });
push({ type: "form.cancelled", data: { id: "frm_child", sessionID: "child" } });
const deadline = Date.now() + 2000;
while (posted.length < 2 && Date.now() < deadline) {
  await new Promise((resolve) => setTimeout(resolve, 20));
}
controller.abort();
notify();
server.close();
console.log(JSON.stringify(posted));
"#;

/// 场景：OpenCode 2 用 form 事件表示原生问题，而不是 1.19.3 的 HTTP question。
/// 构造：root 创建表单后回复；child 已登记为子会话，也创建并取消一张表单。
/// 执行：安装当前插件，把这 5 个事件推进订阅流，读取 POST 出的 provider_event。
/// 预期：root 依次是 QuestionRequest 与 QuestionSettled。
///   负断言：子会话的 form.created / form.cancelled 不发到父卡片。
#[test]
fn opencode_v2_form_events_mark_question_on_parent_only() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("shortcut: 本机没有 node，跳过 OpenCode 插件行为测试");
        return;
    }
    let temp = tempfile::tempdir().expect("应能创建临时目录");
    let plugin_path = temp.path().join("norma-hook.js");
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
        &temp.path().join("norma-hook"),
        &paths,
    )
    .expect("OpenCode 插件应能安装");
    let module = temp.path().join("plugin.mjs");
    std::fs::copy(&plugin_path, &module).expect("应能复制插件");
    let driver = temp.path().join("form-driver.mjs");
    std::fs::write(&driver, FORM_DRIVER).expect("应能写入驱动脚本");
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
        vec!["QuestionRequest:root", "QuestionSettled:root"],
        "父会话的 form 必须变成等待回答，子会话表单不得改父卡片"
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
    let plugin_path = temp.path().join("norma-hook.js");
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
