use super::{agents_ready_within, Pipe, SharedHub};
use crate::acp::client::AcpSession;
use crate::acp::wire::{Incoming, Rpc};
use crate::backend::BackendKind;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 场景：取消会话 A 不能关掉共用连接，也不能让 B 收不到后续消息。
/// 数据构造：一条假连接上挂 sess-a 和 sess-b。
/// 执行过程：只取消 A，再投递一条 B 的 session/update。
/// 预期结果：写出的是 A 的 session/cancel；连接仍活着；B 收到更新，A 收不到。
#[test]
fn cancel_a_leaves_b_and_the_connection() {
    let pipe = Pipe::new();
    let a = pipe.attach("sess-a");
    let mut b = pipe.attach("sess-b");
    a.cancel();
    let sent = pipe.sent();
    assert_eq!(sent.len(), 1, "取消只发一条消息");
    assert_eq!(
        sent[0]["method"], "session/cancel",
        "取消必须是 session/cancel"
    );
    assert_eq!(
        sent[0]["params"]["sessionId"], "sess-a",
        "取消只能带上 A 的 sessionId"
    );
    assert!(pipe.alive(), "取消一个会话不能把整条连接杀掉");
    assert!(
        pipe.deliver(Incoming::Notification {
            method: "session/update".into(),
            params: json!({"sessionId": "sess-b", "update": {"sessionUpdate": "agent_message_chunk"}}),
        }),
        "B 的更新必须还能送进信箱"
    );
    assert!(
        b.recv(Duration::from_millis(50)).is_ok(),
        "取消 A 之后 B 仍能收到自己的更新"
    );
    drop(a);
    assert!(pipe.alive(), "A 的信箱关掉也不能把连接杀掉");
}

/// 场景：OpenCode 不走共用连接，Claude 和 Codex 走。
/// 数据构造：三种后端各调用一次 prepare。
/// 执行过程：调用 SharedHub::prepare。
/// 预期结果：OpenCode 得到 None（每个任务一个进程）；Claude、Codex 得到共用连接。
#[test]
fn only_claude_and_codex_get_a_shared_hub() {
    let make = |kind| {
        SharedHub::prepare(
            kind,
            PathBuf::from("x"),
            Vec::new(),
            PathBuf::from("/"),
            Vec::new(),
        )
    };
    assert!(
        make(BackendKind::OpenCode).is_none(),
        "OpenCode 必须每个任务一个进程"
    );
    assert!(
        make(BackendKind::ClaudeCode).is_some(),
        "Claude 幕僚共用一条连接"
    );
    assert!(make(BackendKind::Codex).is_some(), "Codex 幕僚共用一条连接");
}

fn script(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("norma");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

/// 场景：自检只在 norma 成功退出时才算通过；卡住或被取消时要在限时内返回，不能阻塞建会话。
/// 数据构造：四个假 norma 脚本：exit 0、exit 1、sleep 30、sleep 30 且 cancel 已置位。
/// 执行过程：以 1s 上限逐个调用 agents_ready_within。
/// 预期结果：exit 0 通过；exit 1 不通过；卡住的在 1s 上限附近返回 false（不超过 3s）；被取消的立刻 false。
#[test]
fn agents_check_passes_only_on_success_and_never_hangs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cancel = AtomicBool::new(false);
    // 冷启动 exec 在并发测试里可能超过 300ms；上限留到 1s，仍能证明卡住的自检不会拖住建会话。
    let limit = Duration::from_secs(1);
    let ok = script(dir.path(), "exit 0");
    assert!(
        agents_ready_within(&ok, "tok", dir.path(), &cancel, limit),
        "exit 0 应通过自检"
    );
    let bad = script(dir.path(), "exit 1");
    assert!(
        !agents_ready_within(&bad, "tok", dir.path(), &cancel, limit),
        "exit 1 不能通过自检"
    );
    let hang = script(dir.path(), "sleep 30");
    let started = Instant::now();
    assert!(
        !agents_ready_within(&hang, "tok", dir.path(), &cancel, limit),
        "卡住的自检必须判失败"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "卡住的自检必须在限时内返回，实际 {:?}",
        started.elapsed()
    );
    cancel.store(true, Ordering::Relaxed);
    let started = Instant::now();
    assert!(
        !agents_ready_within(&hang, "tok", dir.path(), &cancel, Duration::from_secs(30)),
        "取消后自检判失败"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "取消后自检要立刻返回，实际 {:?}",
        started.elapsed()
    );
    assert!(
        !agents_ready_within(&ok, "", dir.path(), &AtomicBool::new(false), limit),
        "没有 token 不能通过自检"
    );
}

fn chunk(session: &str, text: &str) -> Incoming {
    Incoming::Notification {
        method: "session/update".into(),
        params: json!({"sessionId": session, "update": {
            "sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}}),
    }
}

fn wait_for_prompt(pipe: &Pipe, count: usize) -> u64 {
    let started = Instant::now();
    loop {
        let prompts: Vec<_> = pipe
            .sent()
            .into_iter()
            .filter(|m| m["method"] == "session/prompt")
            .collect();
        if prompts.len() >= count {
            return prompts[count - 1]["id"].as_u64().expect("prompt id");
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "等不到第 {count} 条 session/prompt"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// 场景：共用连接上取消一轮后，旧一轮的尾巴不能串进下一轮的回复。
/// 数据构造：sess-a 在共用连接上发出第 1 条 prompt 后被取消；取消之后适配器又送来旧轮的
///   一段文字 "OLD"、一条旧轮的权限请求、以及旧 prompt 的 stopReason=cancelled 响应。
/// 执行过程：
///   1. 线程里发第 1 条 prompt，主线程置 cancel，得到 Cancelled。
///   2. 投递 "OLD" 通知、权限请求（id 99）、旧响应。
///   3. 发第 2 条 prompt，投递 "NEW" 和第 2 条的 end_turn 响应。
///
/// 预期结果：
///   - 第 2 轮回复是 "NEW"，不含 "OLD"（尾巴被排干）。
///   - 旧权限请求被回了错误响应（适配器不会一直等）。
///   - 第 2 条 session/prompt 在旧响应被排干之后才发出。
#[test]
fn cancelled_turn_tail_does_not_leak_into_the_next_turn() {
    let pipe = Pipe::new();
    let port = pipe.attach("sess-a");
    let mut session =
        AcpSession::from_shared(port, "sess-a".into(), json!({}), PathBuf::from("/"), None);
    let cancel = Arc::new(AtomicBool::new(false));
    let first = {
        let cancel = cancel.clone();
        std::thread::scope(|scope| {
            let handle = scope
                .spawn(|| session.prompt("one", &cancel, Duration::from_secs(10), &mut |_| {}));
            let old_id = wait_for_prompt(&pipe, 1);
            cancel.store(true, Ordering::Relaxed);
            let result = handle.join().expect("first turn thread");
            (old_id, result)
        })
    };
    let (old_id, result) = first;
    assert!(
        matches!(result, Err(crate::acp::client::AcpError::Cancelled)),
        "第 1 轮应是已取消"
    );
    assert!(pipe.deliver(chunk("sess-a", "OLD")), "旧轮的尾巴能投进信箱");
    assert!(
        pipe.deliver(Incoming::Request {
            id: json!(99),
            method: "session/request_permission".into(),
            params: json!({"sessionId": "sess-a", "toolCall": {"name": "Bash", "rawInput": {"command": "x"}}}),
        }),
        "旧轮的权限请求能投进信箱"
    );
    assert!(
        pipe.deliver(Incoming::Response {
            id: old_id,
            result: Ok(json!({"stopReason": "cancelled"}))
        }),
        "旧 prompt 的响应能投进信箱"
    );
    let cancel = AtomicBool::new(false);
    let reply = std::thread::scope(|scope| {
        let handle =
            scope.spawn(|| session.prompt("two", &cancel, Duration::from_secs(10), &mut |_| {}));
        let new_id = wait_for_prompt(&pipe, 2);
        assert!(
            pipe.deliver(chunk("sess-a", "NEW")),
            "新一轮的文字能投进信箱"
        );
        assert!(
            pipe.deliver(Incoming::Response {
                id: new_id,
                result: Ok(json!({"stopReason": "end_turn"}))
            }),
            "新一轮的响应能投进信箱"
        );
        handle.join().expect("second turn thread")
    });
    assert_eq!(
        reply.expect("第 2 轮应成功"),
        "NEW",
        "第 2 轮回复里不能混进旧轮的 OLD"
    );
    let answered = pipe
        .sent()
        .iter()
        .any(|m| m["id"] == json!(99) && m.get("error").is_some());
    assert!(
        answered,
        "旧轮遗留的权限请求必须被回错误，不能让适配器一直等"
    );
}
