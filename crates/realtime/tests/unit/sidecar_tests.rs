use realtime::{
    RealtimeCommand, RealtimeEvent, RealtimeSessionConfig, RealtimeSessionId, RealtimeSidecarError,
    SecretString, SidecarLaunchConfig, SidecarProcess,
};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

mod callback_tests;

fn fake_sidecar_config(api_key: &str) -> SidecarLaunchConfig {
    SidecarLaunchConfig {
        executable: node_executable(),
        args: vec![fixture_path().to_string_lossy().to_string()],
        openai_api_key: SecretString::new(api_key),
    }
}

fn node_executable() -> PathBuf {
    std::env::var_os("NODE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("node"))
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_sidecar.mjs")
}

fn start_command(model: &str) -> RealtimeCommand {
    RealtimeCommand::StartSession(RealtimeSessionConfig {
        session_id: RealtimeSessionId::new("sidecar-test-session"),
        model: model.to_string(),
        tools: vec!["enqueue_card_task".to_string()],
    })
}

fn stop_command() -> RealtimeCommand {
    RealtimeCommand::StopSession {
        session_id: RealtimeSessionId::new("sidecar-test-session"),
    }
}

fn wait_for_sidecar_event(process: &SidecarProcess) -> Result<RealtimeEvent, RealtimeSidecarError> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(event) = process.try_recv() {
            return event;
        }
        assert!(
            Instant::now() < deadline,
            "sidecar test timed out waiting for an event from fake sidecar stdout"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

/// SC-1: Rust sidecar bridge sends start_session JSONL and receives session_started.
///
/// 数据构造（含关键数值的推导过程）：
///   executable = node（或 NODE env 覆盖），args[0] = tests/fixtures/fake_sidecar.mjs
///   api_key = "sk-sidecar-start-secret"，仅允许进入 OPENAI_API_KEY env
///   session_id = "sidecar-test-session"，model = "gpt-realtime-2"，tools.len = 1
///   wait timeout = 3s，用于区分进程无响应和正常异步 stdout
///
/// 执行过程（逐步说明系统如何处理）：
///   1. SidecarProcess::spawn(fake config) 启动 Node fixture，并管道化 stdin/stdout/stderr
///   2. send(StartSession) 写入一行 JSONL 到 child stdin
///   3. fake sidecar 解析命令后输出 session_started JSON line
///   4. wait_for_sidecar_event 轮询 try_recv，读取解码后的 RealtimeEvent
///   5. 发送 stop_session 并调用 stop，收口 fixture 子进程
///
/// 预期结果：
///   - 正断言：收到 SessionStarted 且 session_id 精确等于 sidecar-test-session
///   - 正断言：stop 能在 3s 内完成
///   - 负断言：start_session 不应被解码为 SessionStopped
#[test]
fn sidecar_start_session_emits_session_started() {
    let mut process = SidecarProcess::spawn(fake_sidecar_config("sk-sidecar-start-secret"))
        .expect("fake sidecar should launch through node");

    process
        .send(&start_command("gpt-realtime-2"))
        .expect("start_session should be written to fake sidecar stdin");

    let event = wait_for_sidecar_event(&process)
        .expect("fake sidecar session_started should decode successfully");
    assert_eq!(
        event,
        RealtimeEvent::SessionStarted {
            session_id: RealtimeSessionId::new("sidecar-test-session"),
        },
        "start_session should produce a matching SessionStarted event"
    );
    assert_ne!(
        event,
        RealtimeEvent::SessionStopped {
            session_id: RealtimeSessionId::new("sidecar-test-session"),
            reason: "user stopped".to_string(),
        },
        "start_session must not be interpreted as an immediate stop event"
    );

    process
        .send(&stop_command())
        .expect("stop_session should be written before stopping fake sidecar");
    process
        .stop(Duration::from_secs(3))
        .expect("fake sidecar should exit after stop_session");
}

/// SC-2: malformed fake sidecar stdout is surfaced as RealtimeSidecarError::MalformedEvent.
///
/// 数据构造（含关键数值的推导过程）：
///   model = "emit-malformed-json"，fixture 分支输出 "{malformed-json" + "\n"
///   malformed line byte_len = 15，非空但不是合法 JSON object
///   wait timeout = 3s，足够覆盖本地 Node 进程启动与 stdout flush
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 fake sidecar
///   2. send(StartSession(model=emit-malformed-json))
///   3. stdout reader 读取 malformed line 并调用协议 decode_event
///   4. 协议错误映射为 sidecar 层 MalformedEvent
///   5. 发送 stop_session 并停止进程
///
/// 预期结果：
///   - 正断言：try_recv 返回 Err(RealtimeSidecarError::MalformedEvent)
///   - 正断言：错误 Display 文本说明 event malformed
///   - 负断言：malformed line 不应被伪装成 SessionStarted
#[test]
fn sidecar_malformed_stdout_returns_malformed_event_error() {
    let mut process = SidecarProcess::spawn(fake_sidecar_config("sk-sidecar-malformed-secret"))
        .expect("fake sidecar should launch for malformed event test");

    process
        .send(&start_command("emit-malformed-json"))
        .expect("start_session should be sent before fake sidecar emits malformed JSON");

    let err = wait_for_sidecar_event(&process)
        .expect_err("malformed fake sidecar line should return a sidecar error");
    assert!(
        matches!(err, RealtimeSidecarError::MalformedEvent),
        "malformed stdout should map to RealtimeSidecarError::MalformedEvent, got {err:?}"
    );
    assert!(
        err.to_string().contains("malformed"),
        "MalformedEvent Display should explain that the sidecar event was malformed: {err}"
    );
    assert_ne!(
        err.to_string(),
        "session_started",
        "malformed stdout must not be reported as a successful session_started event"
    );

    process
        .send(&stop_command())
        .expect("stop_session should still be writable after malformed stdout");
    process
        .stop(Duration::from_secs(3))
        .expect("fake sidecar should exit after malformed event test stop_session");
}

/// SC-3: unknown fake sidecar event is ignored and the next valid event is delivered.
///
/// 数据构造（含关键数值的推导过程）：
///   model = "emit-unknown-event"，fixture 输出 2 行：
///     1. {"type":"mystery","session_id":"sidecar-test-session"}
///     2. {"type":"session_started","session_id":"sidecar-test-session"}
///   delivered events expected = 1，因为 unknown line must be filtered out
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 fake sidecar 并发送 start_session
///   2. stdout reader 解码 mystery 为 IgnoredUnknown 后丢弃
///   3. stdout reader 继续读取下一行 session_started
///   4. try_recv 返回第一个可见事件
///
/// 预期结果：
///   - 正断言：收到的第一个可见事件是 SessionStarted
///   - 正断言：进程仍可接收 stop_session 并退出
///   - 负断言：调用方不应收到 IgnoredUnknown 事件
#[test]
fn sidecar_unknown_event_is_ignored_before_valid_started_event() {
    let mut process = SidecarProcess::spawn(fake_sidecar_config("sk-sidecar-unknown-secret"))
        .expect("fake sidecar should launch for unknown event test");

    process
        .send(&start_command("emit-unknown-event"))
        .expect("start_session should be sent before unknown event fixture branch");

    let event = wait_for_sidecar_event(&process)
        .expect("valid event after unknown fake event should decode successfully");
    assert_eq!(
        event,
        RealtimeEvent::SessionStarted {
            session_id: RealtimeSessionId::new("sidecar-test-session"),
        },
        "unknown sidecar events should be skipped until a valid event is available"
    );
    assert_ne!(
        event,
        RealtimeEvent::IgnoredUnknown {
            event_type: "mystery".to_string(),
        },
        "IgnoredUnknown is an internal sidecar concern and should not be delivered to callers"
    );

    process
        .send(&stop_command())
        .expect("stop_session should be accepted after unknown event was ignored");
    process
        .stop(Duration::from_secs(3))
        .expect("fake sidecar should still exit cleanly after unknown event");
}

/// SC-4: fake sidecar without a terminal event is surfaced as StdoutClosed.
///
/// 数据构造（含关键数值的推导过程）：
///   model = "exit-without-event"，fixture 收到 start_session 后立即 exit(0)
///   stdout lines = 0，channel 会在 reader thread 结束后 disconnected
///   wait timeout = 3s，足够覆盖本地 Node 进程启动、退出和 reader thread 收口
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 fake sidecar
///   2. send(StartSession(model=exit-without-event))
///   3. fake sidecar 不写 session_started/session_stopped，直接退出
///   4. try_recv 观察 stdout channel disconnected 并返回 StdoutClosed
///
/// 预期结果：
///   - 正断言：收到 RealtimeSidecarError::StdoutClosed
///   - 正断言：stop 能处理已经退出的 sidecar
///   - 负断言：异常退出不应被误报为 SessionStarted
#[test]
fn sidecar_exit_without_event_returns_stdout_closed_error() {
    let mut process = SidecarProcess::spawn(fake_sidecar_config("sk-sidecar-exit-secret"))
        .expect("fake sidecar should launch for unexpected-exit test");

    process
        .send(&start_command("exit-without-event"))
        .expect("start_session should be sent before fake sidecar exits");

    let err = wait_for_sidecar_event(&process)
        .expect_err("sidecar exit without a terminal event should surface an error");
    assert!(
        matches!(err, RealtimeSidecarError::StdoutClosed),
        "sidecar stdout close should map to RealtimeSidecarError::StdoutClosed, got {err:?}"
    );
    assert!(
        err.to_string().contains("stdout"),
        "StdoutClosed Display should mention stdout closure: {err}"
    );
    assert_ne!(
        err.to_string(),
        "session_started",
        "sidecar exit without an event must not be reported as SessionStarted"
    );

    process
        .stop(Duration::from_secs(3))
        .expect("stop should tolerate a fake sidecar that has already exited");
}

/// SC-5: stop_session command makes the fake sidecar emit session_stopped and exit.
///
/// 数据构造（含关键数值的推导过程）：
///   session_id = "sidecar-test-session"
///   stop reason = "user stopped"，固定由 fixture 写出
///   timeout = 3s，stop(self, timeout) 应在该预算内 wait 到 child exit
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 fake sidecar，发送 start_session，并消费 session_started
///   2. send(StopSession) 写入 stop_session JSONL
///   3. fake sidecar 输出 session_stopped 后 process.exit(0)
///   4. Rust 读取 session_stopped，并由 stop 等待进程退出
///
/// 预期结果：
///   - 正断言：收到 SessionStopped，reason 精确等于 user stopped
///   - 正断言：stop 返回 Ok，说明 child 已退出或被正常收口
///   - 负断言：stop_session 后不应再报告 SessionStarted
#[test]
fn sidecar_stop_session_emits_stopped_and_exits() {
    let mut process = SidecarProcess::spawn(fake_sidecar_config("sk-sidecar-stop-secret"))
        .expect("fake sidecar should launch for stop_session test");
    process
        .send(&start_command("gpt-realtime-2"))
        .expect("start_session should be sent before stop_session");
    let started = wait_for_sidecar_event(&process)
        .expect("fake sidecar should acknowledge start before stop test");
    assert_eq!(
        started,
        RealtimeEvent::SessionStarted {
            session_id: RealtimeSessionId::new("sidecar-test-session"),
        },
        "fixture should enter a started state before stop_session is tested"
    );

    process
        .send(&stop_command())
        .expect("stop_session should be written to fake sidecar stdin");
    let stopped = wait_for_sidecar_event(&process)
        .expect("fake sidecar should emit session_stopped before exiting");

    assert_eq!(
        stopped,
        RealtimeEvent::SessionStopped {
            session_id: RealtimeSessionId::new("sidecar-test-session"),
            reason: "user stopped".to_string(),
        },
        "stop_session should produce the fixture's exact SessionStopped event"
    );
    assert_ne!(
        stopped,
        RealtimeEvent::SessionStarted {
            session_id: RealtimeSessionId::new("sidecar-test-session"),
        },
        "stop_session must not be surfaced as another SessionStarted event"
    );
    process
        .stop(Duration::from_secs(3))
        .expect("stop should observe fake sidecar process exit after stop_session");
}

/// SC-6: sidecar config, launch errors, and formatted logs never expose the API key string.
///
/// 数据构造（含关键数值的推导过程）：
///   secret = "sk-sidecar-leak-check-secret"
///   bad executable = "__definitely_missing_realtime_sidecar__"，spawn 必然返回 OS error
///   args = ["--model", "gpt-realtime-2"]，不含 secret，API key 只能走 OPENAI_API_KEY env
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 SidecarLaunchConfig，检查 Debug 格式
///   2. 格式化 SecretString 的 Debug 与 Display
///   3. spawn bad executable，获取 launch error
///   4. 检查 Display/Debug 文本均不包含 secret
///
/// 预期结果：
///   - 正断言：SecretString 以 redacted 形式格式化
///   - 正断言：launch error Display/Debug 可读但不含 API key
///   - 负断言：config args、config Debug、error Display/Debug 都不应包含 secret 原文
#[test]
fn sidecar_launch_config_and_errors_do_not_leak_api_key() {
    let secret = "sk-sidecar-leak-check-secret";
    let config = SidecarLaunchConfig {
        executable: PathBuf::from("__definitely_missing_realtime_sidecar__"),
        args: vec!["--model".to_string(), "gpt-realtime-2".to_string()],
        openai_api_key: SecretString::new(secret),
    };

    assert!(
        !config.args.iter().any(|arg| arg.contains(secret)),
        "sidecar launch args must never contain the API key; it belongs only in OPENAI_API_KEY env"
    );
    assert!(
        !format!("{config:?}").contains(secret),
        "SidecarLaunchConfig Debug output must redact the API key"
    );
    assert!(
        !format!("{:?}", config.openai_api_key).contains(secret),
        "SecretString Debug output must redact the API key"
    );
    assert!(
        !format!("{}", config.openai_api_key).contains(secret),
        "SecretString Display output must redact the API key"
    );
    assert_ne!(
        format!("{}", config.openai_api_key),
        secret,
        "SecretString Display must not equal the raw API key"
    );

    let err = SidecarProcess::spawn(config)
        .expect_err("missing sidecar executable should produce a launch error");
    assert!(
        !err.to_string().contains(secret),
        "sidecar launch error Display must not leak the API key: {err}"
    );
    assert!(
        !format!("{err:?}").contains(secret),
        "sidecar launch error Debug must not leak the API key: {err:?}"
    );
}
