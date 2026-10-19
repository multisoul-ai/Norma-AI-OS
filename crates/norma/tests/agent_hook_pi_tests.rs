use norma::agent_hook::{
    agent_hooks_installed_for_source, default_install_paths, install_hook_config,
    observe_conversation, parse_hook_event, task_state_for_provider_event, uninstall_hook_config,
    AgentHookTaskState, ConversationObservationKind, HookInstallPaths, HookSource, HookTarget,
};
use norma::protocol::AgentProfile;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;

fn paths_in(root: &Path) -> HookInstallPaths {
    HookInstallPaths {
        claude_settings: root.join("claude.json"),
        codex_hooks: root.join("codex.json"),
        cursor_hooks: root.join("cursor.json"),
        grok_hooks: root.join("grok.json"),
        opencode_plugin: root.join("opencode.js"),
        infcode_settings: root.join("infcode-settings.json"),
        pi_extension: root.join("pi-agent/extensions/norma-agent-hook.ts"),
        kimi_config: root.join("kimi-home").join("config.toml"),
        hermes_plugin_dir: root.join("hermes/plugins/norma-agent-hook"),
        zcode_config: root.join("zcode").join("config.json"),
    }
}

/// Pi 扩展回传的 Stop 事件归一化为 Pi profile，且携带完整回复用于对话证据。
///
/// 数据构造：node_id=31；raw 为扩展产出的 JSON：
///   hook_event_name=Stop，session_id=UUID（36 字符），last_assistant_message="PI_FIXTURE_DONE"。
///
/// 执行过程：parse_hook_event(Pi) -> task_state_for_provider_event -> observe_conversation。
///
/// 预期结果：
///   - 正断言：profile=Pi，状态=Done，对话证据为 CompleteAgentReply 且文本一致。
///   - 负断言：profile 不得是 OpenCode；回复证据不得是 Unavailable。
#[test]
fn pi_stop_event_maps_to_done_with_reply_evidence() {
    let raw = json!({
        "hook_event_name": "Stop",
        "session_id": "01a14cb5-4b48-71eb-af74-53296bd903ff",
        "last_assistant_message": "PI_FIXTURE_DONE"
    });

    let event = parse_hook_event(HookSource::Pi, 31, raw).expect("Pi Stop 应可解析");
    let observation = observe_conversation(&event).expect("Stop 应产生对话观察");

    assert_eq!(event.profile, AgentProfile::Pi, "profile 应为 Pi");
    assert_ne!(event.profile, AgentProfile::OpenCode, "不得误标为 OpenCode");
    assert_eq!(
        task_state_for_provider_event(&event),
        Some(AgentHookTaskState::Done),
        "Stop 应映射为 Done"
    );
    assert_eq!(
        observation.kind,
        ConversationObservationKind::CompleteAgentReply
    );
    assert_eq!(observation.text.as_deref(), Some("PI_FIXTURE_DONE"));
}

/// Pi 的五种状态事件映射：Ready / Thinking / RunningTool / Failed，工具失败不等于任务失败。
///
/// 数据构造：依次解析 SessionStart、UserPromptSubmit、PreToolUse(Bash)、PostToolUseFailure、StopFailure(message)。
///
/// 预期结果：
///   - 正断言：Ready、Thinking、RunningTool(Bash)、Thinking（工具失败后仍继续）、Failed(message)。
///   - 负断言：PostToolUseFailure 不得映射为 Failed。
#[test]
fn pi_events_map_to_five_states() {
    let map = |name: &str, extra: serde_json::Value| {
        let mut raw = json!({"hook_event_name": name, "session_id": "s"});
        raw.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let event = parse_hook_event(HookSource::Pi, 1, raw).expect("应可解析");
        task_state_for_provider_event(&event)
    };

    assert_eq!(
        map("SessionStart", json!({})),
        Some(AgentHookTaskState::Ready)
    );
    assert_eq!(
        map("UserPromptSubmit", json!({"prompt": "hi"})),
        Some(AgentHookTaskState::Thinking)
    );
    assert_eq!(
        map("PreToolUse", json!({"tool_name": "Bash"})),
        Some(AgentHookTaskState::RunningTool {
            tool_name: Some("Bash".to_string())
        })
    );
    let failure = map("PostToolUseFailure", json!({"message": "x"}));
    assert_eq!(
        failure,
        Some(AgentHookTaskState::Thinking),
        "工具失败后任务仍在进行"
    );
    assert!(
        !matches!(failure, Some(AgentHookTaskState::Failed { .. })),
        "工具失败不得判整项任务 Failed"
    );
    assert_eq!(
        map("StopFailure", json!({"message": "boom"})),
        Some(AgentHookTaskState::Failed {
            message: "boom".to_string()
        })
    );
}

/// 安装、重复安装（升级）、卸载只管理带 marker 的自有文件，拒绝覆盖非自有同名文件。
///
/// 数据构造：临时目录；先装一次，再把文件改成旧版本（带 marker）模拟升级；
///   再造另一个路径放无 marker 的用户文件。
///
/// 预期结果：
///   - 正断言：安装后文件含 marker 且 installed=true；升级覆盖为当前内容；卸载后文件消失。
///   - 负断言：用户文件安装报错且内容不变；卸载不删除用户文件；其他扩展文件不受影响。
#[test]
fn pi_extension_install_upgrade_uninstall_respects_ownership() {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths_in(temp.path());
    let ext = paths.pi_extension.clone();
    let sibling = ext.parent().unwrap().join("other.ts");
    std::fs::create_dir_all(ext.parent().unwrap()).unwrap();
    std::fs::write(&sibling, "// user extension").unwrap();
    assert!(!agent_hooks_installed_for_source(HookSource::Pi, &paths));

    install_hook_config(HookTarget::Pi, Path::new("unused"), &paths).expect("安装应成功");
    let installed = std::fs::read_to_string(&ext).unwrap();
    assert!(
        installed.contains("multisoul-pi-extension-marker"),
        "应含 marker"
    );
    assert!(
        installed.contains("MULTISOUL_HOOK_ENDPOINT"),
        "应读取 Norma endpoint env"
    );
    assert!(agent_hooks_installed_for_source(HookSource::Pi, &paths));

    std::fs::write(&ext, "// multisoul-pi-extension-marker\n// old").unwrap();
    install_hook_config(HookTarget::Pi, Path::new("unused"), &paths).expect("升级应成功");
    assert_eq!(
        std::fs::read_to_string(&ext).unwrap(),
        installed,
        "升级应替换为当前内容"
    );

    uninstall_hook_config(HookTarget::Pi, &paths).expect("卸载应成功");
    assert!(!ext.exists(), "自有文件应被删除");
    assert!(sibling.exists(), "其他扩展不得被删除");

    std::fs::write(&ext, "// user's own file").unwrap();
    let error = install_hook_config(HookTarget::Pi, Path::new("unused"), &paths);
    assert!(error.is_err(), "非自有同名文件必须拒绝覆盖");
    assert_eq!(std::fs::read_to_string(&ext).unwrap(), "// user's own file");
    assert!(
        !agent_hooks_installed_for_source(HookSource::Pi, &paths),
        "无 marker 不算已安装"
    );
    uninstall_hook_config(HookTarget::Pi, &paths).unwrap();
    assert!(ext.exists(), "卸载不得删除非自有文件");
}

/// 默认路径：未设置 PI_CODING_AGENT_DIR 时落在 ~/.pi/agent/extensions/norma-agent-hook.ts。
///
/// 数据构造：读取 default_install_paths，仅在当前进程未设置该环境变量时断言默认形状。
///
/// 预期结果：路径以 `.pi/agent/extensions/norma-agent-hook.ts` 结尾；不得落在 Grok 目录。
#[test]
fn pi_default_path_is_pi_agent_extensions() {
    if std::env::var_os("PI_CODING_AGENT_DIR").is_some() {
        return;
    }
    let path = default_install_paths().pi_extension;
    assert!(
        path.ends_with(".pi/agent/extensions/norma-agent-hook.ts"),
        "默认路径错误：{}",
        path.display()
    );
    assert!(
        !path.to_string_lossy().contains(".grok"),
        "不得串到 Grok 路径"
    );
}

const HARNESS: &str = r#"
import { pathToFileURL } from "node:url";
const mod = await import(pathToFileURL(process.argv[2]).href);
const handlers = new Map();
mod.default({ on: (name, fn) => handlers.set(name, fn) });
const ctx = { cwd: "/work", sessionManager: { getSessionId: () => "SID-1" } };
const run = (name, event) => handlers.get(name)(event, ctx);
const scenario = process.argv[3];
run("session_start", { type: "session_start", reason: "startup" });
run("before_agent_start", { type: "before_agent_start", prompt: "do it" });
run("tool_call", { type: "tool_call", toolName: "bash", toolCallId: "c1", input: { command: "ls" } });
run("tool_result", { type: "tool_result", toolName: "bash", isError: true, content: [] });
run("turn_end", { type: "turn_end", message: { role: "assistant", stopReason: "toolUse", content: [{ type: "text", text: "TOOL-TURN-TEXT" }] } });
if (scenario === "error") {
  run("turn_end", { type: "turn_end", message: { role: "assistant", stopReason: "error", errorMessage: "400: boom", content: [] } });
  run("agent_settled", { type: "agent_settled", aborted: false });
} else if (scenario === "aborted") {
  run("agent_settled", { type: "agent_settled", aborted: true });
} else {
  run("turn_end", { type: "turn_end", message: { role: "assistant", stopReason: "stop", content: [{ type: "text", text: "FINAL" }] } });
  run("agent_settled", { type: "agent_settled", aborted: false });
}
const reasons = scenario === "reloads" ? ["reload", "new", "resume", "fork"] : ["quit"];
for (const reason of reasons) run("session_shutdown", { type: "session_shutdown", reason });
await new Promise((r) => setTimeout(r, 800));
"#;

fn run_extension(scenario: &str, with_env: bool) -> Vec<serde_json::Value> {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths_in(temp.path());
    install_hook_config(HookTarget::Pi, Path::new("unused"), &paths).unwrap();
    let ext_mjs = temp.path().join("ext.mjs");
    std::fs::copy(&paths.pi_extension, &ext_mjs).unwrap();
    let harness = temp.path().join("harness.mjs");
    std::fs::write(&harness, HARNESS).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buffer = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = stream.read(&mut chunk).unwrap_or(0);
                buffer.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buffer).to_string();
                if let Some(split) = text.find("\r\n\r\n") {
                    let length = text[..split]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if buffer.len() >= split + 4 + length || n == 0 {
                        let head = text[..split].to_string();
                        let body = buffer[split + 4..].to_vec();
                        let _ = stream.write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                        let _ = tx.send((head, body));
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
        }
    });

    let mut command = Command::new("node");
    command.arg(&harness).arg(&ext_mjs).arg(scenario);
    if with_env {
        command
            .env("MULTISOUL_NODE_ID", "77")
            .env(
                "MULTISOUL_HOOK_ENDPOINT",
                format!("http://127.0.0.1:{port}/hook"),
            )
            .env("MULTISOUL_HOOK_TOKEN", "tok-1");
    } else {
        command
            .env_remove("MULTISOUL_NODE_ID")
            .env_remove("MULTISOUL_HOOK_ENDPOINT")
            .env_remove("MULTISOUL_HOOK_TOKEN");
    }
    let status = command.status().expect("node 应可运行");
    assert!(status.success(), "扩展加载与回调不得抛错（失败开放）");
    let mut events = Vec::new();
    while let Ok((head, body)) = rx.recv_timeout(std::time::Duration::from_millis(300)) {
        assert!(head.contains("Bearer tok-1"), "应带 Bearer token：{head}");
        events.push(serde_json::from_slice(&body).expect("body 应是 JSON"));
    }
    events
}

fn names(events: &[serde_json::Value]) -> Vec<String> {
    events
        .iter()
        .map(|e| e["provider_event"].as_str().unwrap().to_string())
        .collect()
}

/// 真实 node 运行安装出来的扩展：成功路径按 Pi 事件顺序转发，Stop 只带最终回复。
///
/// 数据构造：桩 pi 依次触发 session_start、before_agent_start(prompt="do it")、tool_call(bash)、
///   tool_result(isError=true)、turn_end(toolUse，文本 TOOL-TURN-TEXT)、turn_end(stop，FINAL)、
///   agent_settled(aborted=false)、session_shutdown；本地 TCP 服务接收 POST。node_id=77。
///
/// 预期结果：
///   - 正断言：事件序列 SessionStart,UserPromptSubmit,PreToolUse,PostToolUseFailure,Stop,SessionEnd；
///     profile=Pi、node_id=77、session_id=SID-1；Stop 的 last_assistant_message=FINAL；PreToolUse 工具名 Bash。
///   - 负断言：回复不得是工具回合文本；不得出现 StopFailure。
#[test]
fn pi_extension_forwards_success_run_in_order() {
    let events = run_extension("ok", true);

    assert_eq!(
        names(&events),
        [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUseFailure",
            "Stop",
            "SessionEnd"
        ],
        "事件顺序应与 Pi 一致"
    );
    assert!(events
        .iter()
        .all(|e| e["profile"] == "Pi" && e["node_id"] == 77 && e["session_id"] == "SID-1"));
    assert_eq!(events[1]["raw_json"]["prompt"], "do it");
    assert_eq!(events[2]["tool_name"], "Bash");
    assert_eq!(events[4]["raw_json"]["last_assistant_message"], "FINAL");
    assert_ne!(
        events[4]["raw_json"]["last_assistant_message"], "TOOL-TURN-TEXT",
        "不得把工具回合文本当最终回复"
    );
    assert!(!names(&events).contains(&"StopFailure".to_string()));
    for event in &events {
        let parsed = parse_hook_event(HookSource::Pi, 77, event["raw_json"].clone());
        assert!(parsed.is_ok(), "Rust 侧应能解析扩展 raw_json：{event}");
    }
}

/// turn 出错后 agent_settled(aborted=false) 仍必须 StopFailure；用户取消也是 StopFailure。
///
/// 数据构造：error 场景 turn_end.stopReason=error，errorMessage="400: boom"，settled aborted=false；
///   aborted 场景 settled aborted=true。
///
/// 预期结果：
///   - 正断言：两个场景的收尾事件都是 StopFailure，error 场景 message 含 boom。
///   - 负断言：不得发出 Stop（不得把失败误报 Done）。
#[test]
fn pi_extension_reports_failure_and_abort_as_stop_failure() {
    let failed = run_extension("error", true);
    let aborted = run_extension("aborted", true);

    assert!(
        names(&failed).contains(&"StopFailure".to_string()),
        "错误应为 StopFailure"
    );
    assert!(
        !names(&failed).contains(&"Stop".to_string()),
        "错误不得发 Stop"
    );
    let failure = failed
        .iter()
        .find(|e| e["provider_event"] == "StopFailure")
        .unwrap();
    assert!(failure["message"].as_str().unwrap().contains("boom"));
    assert!(
        names(&aborted).contains(&"StopFailure".to_string()),
        "取消应为 StopFailure"
    );
    assert!(
        !names(&aborted).contains(&"Stop".to_string()),
        "取消不得发 Stop"
    );
}

/// Pi 在 reload/new/resume/fork 时进程仍存活，也会触发 session_shutdown，不得误报 SessionEnd。
///
/// 数据构造：桩 pi 走成功路径，但收尾依次触发 reason=reload、new、resume、fork 四次 session_shutdown
///   （reason 取值来自 pi-coding-agent 的 SessionShutdownEvent 类型定义）。
///
/// 预期结果：
///   - 正断言：仍收到 SessionStart 与 Stop，证明扩展在工作。
///   - 负断言：事件中不得出现 SessionEnd；quit 场景的 SessionEnd 由顺序测试覆盖。
#[test]
fn pi_extension_ignores_non_quit_session_shutdown() {
    let events = run_extension("reloads", true);

    assert!(
        names(&events).contains(&"Stop".to_string()),
        "应仍转发 Stop"
    );
    assert!(
        !names(&events).contains(&"SessionEnd".to_string()),
        "非 quit 的 session_shutdown 不得标记 Ended：{:?}",
        names(&events)
    );
}

/// 缺少 Norma 环境变量或 Norma 不可达时失败开放：不发送、不抛错、不阻塞。
///
/// 数据构造：with_env=false 移除三个环境变量，桩 pi 照常触发全部事件。
///
/// 预期结果：node 退出码 0；本地服务收到 0 个事件。
#[test]
fn pi_extension_is_silent_without_norma_env() {
    let events = run_extension("ok", false);

    assert!(events.is_empty(), "无环境变量不得发送任何事件");
}
