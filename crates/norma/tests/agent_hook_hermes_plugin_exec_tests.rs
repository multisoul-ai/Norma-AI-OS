//! 用真实 python3 加载并执行 Hermes 插件，本地 TCP 服务接收 POST，验证行为而非文本。

use norma::agent_hook::{
    install_hook_config, uninstall_hook_config_except_hermes, HookInstallPaths, HookTarget,
};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const PLUGIN_SRC: &str = include_str!("../src/agent_hook/hermes_plugin.py");

/// 桩宿主：ctx 仅记录 register_hook；按场景驱动回调后等待发送线程结束。
const DRIVER: &str = r#"
import importlib.util, sys, threading, time
spec = importlib.util.spec_from_file_location("norma_plugin", sys.argv[1])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
hooks = {}
class Ctx:
    def register_hook(self, name, cb):
        hooks[name] = cb
mod.register(Ctx())
extra = {"conversation_history": [{"role": "user", "content": "SECRET"}], "api_key": "K", "cwd": "/x"}
def call(name, **kw):
    kw.update(extra)
    hooks[name](**kw)
call("on_session_start", session_id="s1")
call("pre_llm_call", session_id="s1")
call("pre_tool_call", session_id="s1", tool_name="terminal")
call("post_tool_call", session_id="s1", tool_name="terminal")
call("on_human_input_request", session_id="s1", kind="approval")
call("on_human_input_request", session_id="s1", kind="clarify")
call("on_human_input_request", session_id="s2", kind="approval")
call("on_human_input_resolved", session_id="s1")
call("api_request_error", session_id="s1", retryable=True, reason="rate")
call("api_request_error", session_id="s1", reason="missing-flag")
call("api_request_error", session_id="s1", retryable=False, reason="auth")
call("post_llm_call", session_id="s1", assistant_response="x" * 2000)
call("on_session_end", session_id="s1", interrupted=True)
call("on_session_end", session_id="s3")
call("on_session_finalize", session_id="s1")
deadline = time.time() + 5
while threading.active_count() > 1 and time.time() < deadline:
    time.sleep(0.05)
"#;

struct Received {
    auth: String,
    body: Value,
}

fn serve(listener: TcpListener, tx: mpsc::Sender<Received>) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    data.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&data).to_string();
                    if let Some(split) = text.find("\r\n\r\n") {
                        let len = text
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if data.len() >= split + 4 + len || n == 0 {
                            let auth = text
                                .lines()
                                .find_map(|l| l.strip_prefix("Authorization: "))
                                .unwrap_or_default()
                                .to_string();
                            let body =
                                serde_json::from_slice(&data[split + 4..]).unwrap_or(Value::Null);
                            let _ = tx.send(Received { auth, body });
                            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                            return;
                        }
                    }
                    if n == 0 {
                        return;
                    }
                }
            });
        }
    });
}

fn run_plugin(dir: &Path, endpoint: &str, proxy: Option<&str>) {
    let plugin = dir.join("plugin.py");
    let driver = dir.join("driver.py");
    std::fs::write(&plugin, PLUGIN_SRC).unwrap();
    std::fs::write(&driver, DRIVER).unwrap();
    let mut cmd = Command::new("python3");
    cmd.arg("-I").arg(&driver).arg(&plugin);
    cmd.env("MULTISOUL_HOOK_ENDPOINT", endpoint)
        .env("MULTISOUL_HOOK_TOKEN", "tok-123")
        .env("MULTISOUL_NODE_ID", "node-1")
        .env_remove("no_proxy")
        .env_remove("NO_PROXY");
    if let Some(proxy) = proxy {
        cmd.env("http_proxy", proxy).env("HTTP_PROXY", proxy);
    }
    let out = cmd.output().expect("python3 应可运行");
    assert!(
        out.status.success(),
        "驱动脚本应成功: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn collect(rx: &mpsc::Receiver<Received>, want: usize) -> Vec<Received> {
    let end = Instant::now() + Duration::from_secs(5);
    let mut got = Vec::new();
    while got.len() < want && Instant::now() < end {
        if let Ok(r) = rx.recv_timeout(Duration::from_millis(100)) {
            got.push(r);
        }
    }
    got
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("norma-hermes-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn find<'a>(all: &'a [Received], event: &str, sid: Option<&str>) -> Vec<&'a Received> {
    all.iter()
        .filter(|r| {
            r.body["hook_event_name"] == event && sid.is_none_or(|s| r.body["session_id"] == s)
        })
        .collect()
}

/// 场景：真实 python3 加载插件，按固定序列触发各 Hermes 回调。
/// 数据：s1 走完整流程并混入 conversation_history/api_key/cwd 等额外字段；s2 无工具缓存；s3 无回复。
/// 预期：事件映射正确；retryable=True 与缺省不产生 StopFailure；PermissionRequest 带 s1 缓存的
/// 工具名而 s2 不带（全局缓存的错误实现会失败）；Stop 带截断到 1201 的回复；载荷只含白名单字段。
#[test]
fn hermes_plugin_executes_and_maps_events_to_payloads() {
    let dir = temp_dir("exec");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/hook", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    serve(listener, tx);
    run_plugin(&dir, &endpoint, None);
    // register 的 SessionStart + 回调：1+1+1+1+1+1+1(s2)+1(resolved)+1(StopFailure)+2(Stop)+1(End)+1(start)=12
    let all = collect(&rx, 12);
    assert_eq!(all.len(), 12, "应收到 12 个事件，实际 {}", all.len());
    assert!(
        all.iter().all(|r| r.auth == "Bearer tok-123"),
        "每个请求都应带 Bearer token"
    );
    let allowed = [
        "hook_event_name",
        "session_id",
        "tool_name",
        "message",
        "last_assistant_message",
        "interrupted",
    ];
    for r in &all {
        for key in r.body.as_object().unwrap().keys() {
            assert!(
                allowed.contains(&key.as_str()),
                "载荷出现非白名单字段 {key}"
            );
        }
    }
    assert!(
        !all.iter().any(|r| r.body.to_string().contains("SECRET")),
        "不得包含 conversation_history 内容"
    );
    assert!(
        !all.iter().any(|r| r.body.to_string().contains("api_key")),
        "不得转发 api_key"
    );
    assert_eq!(
        find(&all, "PreToolUse", Some("s1"))[0].body["tool_name"],
        "terminal",
        "PreToolUse 带工具名"
    );
    assert_eq!(
        find(&all, "UserPromptSubmit", Some("s1")).len(),
        1,
        "pre_llm_call 映射 UserPromptSubmit"
    );
    let perm1 = find(&all, "PermissionRequest", Some("s1"));
    assert_eq!(perm1.len(), 1, "kind=clarify 不应产生 PermissionRequest");
    assert_eq!(
        perm1[0].body["tool_name"], "terminal",
        "s1 应使用缓存的工具名"
    );
    let perm2 = find(&all, "PermissionRequest", Some("s2"));
    assert_eq!(perm2.len(), 1, "s2 的审批请求应发出");
    assert!(
        perm2[0].body.get("tool_name").is_none(),
        "s2 没有缓存，不应借用 s1 的工具名"
    );
    let fails = find(&all, "StopFailure", None);
    assert_eq!(
        fails.len(),
        1,
        "retryable=True 与缺省都不应产生 StopFailure"
    );
    assert_eq!(fails[0].body["message"], "auth", "StopFailure 携带 reason");
    let stop1 = find(&all, "Stop", Some("s1"));
    assert_eq!(
        stop1[0].body["last_assistant_message"]
            .as_str()
            .unwrap()
            .len(),
        1201,
        "回复截断到 1201"
    );
    assert_eq!(
        stop1[0].body["interrupted"],
        json!(true),
        "interrupted 透传"
    );
    assert!(
        find(&all, "Stop", Some("s3"))[0]
            .body
            .get("last_assistant_message")
            .is_none(),
        "无回复不应带该字段"
    );
    assert_eq!(
        find(&all, "SessionEnd", Some("s1")).len(),
        1,
        "finalize 映射 SessionEnd"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 场景：用户环境设置了 http_proxy/HTTP_PROXY 指向本地另一个监听端口，且 no_proxy 为空。
/// 预期：请求（含 token）直达目标服务；代理端口 0 个连接。
#[test]
fn hermes_plugin_ignores_proxy_environment() {
    let dir = temp_dir("proxy");
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/hook", target.local_addr().unwrap());
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    serve(target, tx);
    run_plugin(&dir, &endpoint, Some(&proxy_url));
    let all = collect(&rx, 12);
    assert!(!all.is_empty(), "请求应直达目标服务");
    assert!(
        all.iter().all(|r| r.auth == "Bearer tok-123"),
        "目标服务应收到 token"
    );
    assert!(proxy.accept().is_err(), "代理端口不应收到任何连接");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 场景：HookTarget::All 卸载时 Hermes 目录须保留给后台 disable。
/// 预期：uninstall_hook_config_except_hermes(All) 删除其他目标，但 Hermes 插件目录仍在；
/// 对照：其他目标文件已被移除。
#[test]
fn all_uninstall_keeps_hermes_dir_for_disable() {
    let root = temp_dir("order");
    let paths = HookInstallPaths {
        claude_settings: root.join("claude.json"),
        codex_hooks: root.join("codex.json"),
        cursor_hooks: root.join("cursor.json"),
        grok_hooks: root.join("grok.json"),
        opencode_plugin: root.join("opencode.js"),
        hermes_plugin_dir: root.join("hermes/plugins/norma-agent-hook"),
        pi_extension: root.join("pi-agent/extensions/norma-agent-hook.ts"),
        kimi_config: root.join("kimi-home").join("config.toml"),
        zcode_config: root.join("zcode").join("config.json"),
    };
    install_hook_config(HookTarget::All, Path::new("/bin/norma-hook"), &paths).unwrap();
    assert!(
        paths.hermes_plugin_dir.join("__init__.py").exists(),
        "安装后插件应存在"
    );
    uninstall_hook_config_except_hermes(HookTarget::All, &paths).unwrap();
    assert!(
        paths.hermes_plugin_dir.join("__init__.py").exists(),
        "All 卸载不得同步删除 Hermes 目录"
    );
    assert!(
        !norma::agent_hook::agent_hooks_installed_for_source(
            norma::agent_hook::HookSource::OpenCode,
            &paths
        ),
        "OpenCode 应已卸载"
    );
    let _ = std::fs::remove_dir_all(&root);
}
