use super::*;
use serde_json::{json, Value};
use std::{
    os::unix::net::{UnixListener, UnixStream},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket};

fn read_json(socket: &mut WebSocket<UnixStream>) -> Value {
    serde_json::from_str(
        socket
            .read()
            .expect("mock Codex 应收到请求")
            .to_text()
            .expect("mock Codex 请求应为文本帧"),
    )
    .expect("mock Codex 请求应为 JSON")
}

fn serve_connection(listener: &UnixListener, workspace: &Path, reply: &str) {
    let (stream, _) = listener.accept().expect("mock Codex 应接受连接");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("mock Codex 应设置读取超时");
    let mut socket = tungstenite::accept(stream).expect("mock Codex 应完成 WebSocket 握手");
    let initialize = read_json(&mut socket);
    assert_eq!(
        initialize["method"], "initialize",
        "归档读取必须先初始化 Codex 协议"
    );
    socket
        .send(Message::Text(
            json!({"id":initialize["id"],"result":{}})
                .to_string()
                .into(),
        ))
        .expect("mock Codex 应确认初始化");
    let initialized = read_json(&mut socket);
    assert_eq!(
        initialized["method"], "initialized",
        "归档读取必须完成 Codex 初始化握手"
    );
    let metadata = read_json(&mut socket);
    assert_eq!(
        metadata["method"], "thread/read",
        "归档读取必须先校验会话所属项目"
    );
    socket
        .send(Message::Text(
            json!({"id":metadata["id"],"result":{"thread":{
                "id":"session-a","cwd":workspace
            }}})
            .to_string()
            .into(),
        ))
        .expect("mock Codex 应返回会话元数据");
    let history = read_json(&mut socket);
    assert_eq!(
        history["method"], "thread/turns/list",
        "归档读取必须请求完整回合"
    );
    socket
        .send(Message::Text(
            json!({"id":history["id"],"result":{
                "data":[{"id":"turn-a","itemsView":"full","items":[{
                    "id":format!("message-{reply}"),"type":"agentMessage","text":reply
                }]}],
                "nextCursor":null
            }})
            .to_string()
            .into(),
        ))
        .expect("mock Codex 应返回完整历史");
    socket.close(None).expect("mock Codex 应主动断开连接");
}

/// Codex 归档连接断开后，下一次读取必须建立新连接并恢复对话。
///
/// 数据构造（含关键数值的推导过程）：
///   connection-1 = 1 个完整回合，回复为 "first"
///   connection-2 = 1 个完整回合，回复为 "second"
///   RPC deadline = 2s（本地 Unix socket，无网络；高于 1s 断线观察上限）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 首次 read → 建立 connection-1，返回 first，服务端随后主动断开
///   2. 等待客户端确认断线 → 缓存仍持有旧 CodexConnection
///   3. 再次 read → 应丢弃旧连接，建立 connection-2，返回 second
///
/// 预期结果：
///   - 首次读取存在 first，证明测试先经过正常路径
///   - 第二次读取存在 second，证明断线后已重新连接
///   - 第二次读取不保留 first，证明没有复用失效缓存
#[test]
fn codex_archive_reconnects_after_cached_connection_disconnects() {
    let directory = tempfile::tempdir().expect("应创建隔离 Unix socket 目录");
    let socket_path = directory.path().join("codex.sock");
    let listener = UnixListener::bind(&socket_path).expect("应绑定 mock Codex socket");
    let workspace = directory.path().to_path_buf();
    let server_workspace = workspace.clone();
    let server = thread::spawn(move || {
        serve_connection(&listener, &server_workspace, "first");
        serve_connection(&listener, &server_workspace, "second");
    });
    let connect_path = socket_path.clone();
    let codex = Arc::new(CodexReader {
        connection: Mutex::new(None),
        control: Mutex::new(Arc::new(launch::LaunchControl::default())),
        connector: Some(Arc::new(move || CodexConnection::connect(&connect_path))),
        #[cfg(feature = "e2e-test-support")]
        isolated_config: None,
    });
    let reader = MobileArchiveRead {
        workspace: workspace.clone(),
        codex: codex.clone(),
        control: Arc::new(launch::LaunchControl::default()),
    };
    let saved = storage::RecordedMobileSession {
        sequence: 1,
        recorded_at_unix: 1,
        session: storage::MobileSession {
            key: storage::MobileSessionKey {
                node_id: 1,
                profile: "Codex".into(),
                run_id: "run-a".into(),
                session_id: "session-a".into(),
            },
            workspace_root: workspace,
            title: "Codex".into(),
        },
        transcript_path: None,
    };

    // deadline = now + 2s；本地两次 RPC 远低于 1s，额外 1s 覆盖线程调度。
    let first = reader
        .read(saved.clone(), None, Instant::now() + Duration::from_secs(2))
        .expect("首次归档读取应成功");
    assert_eq!(first.items.len(), 1, "首次读取应返回一个完整消息");
    assert_eq!(first.items[0].text, "first", "首次读取应来自第一条连接");
    assert_ne!(
        first.items[0].text, "second",
        "首次读取不得提前使用第二条连接"
    );

    // 1s = 200 × 5ms；等待连接 worker 观察服务端 Close 帧，避免竞态。
    let disconnected_at = Instant::now() + Duration::from_secs(1);
    while codex
        .connection
        .lock()
        .expect("Codex 连接缓存不应中毒")
        .as_ref()
        .is_some_and(|connection| connection.is_connected())
        && Instant::now() < disconnected_at
    {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        codex
            .connection
            .lock()
            .expect("Codex 连接缓存不应中毒")
            .as_ref()
            .is_some_and(|connection| !connection.is_connected()),
        "测试必须先确认缓存中的第一条连接已经断开"
    );

    // deadline = now + 2s；重连与两次本地 RPC 共用同一明确预算。
    let second = reader
        .read(saved, None, Instant::now() + Duration::from_secs(2))
        .expect("断线后的第二次归档读取应自动恢复");
    assert_eq!(second.items.len(), 1, "恢复后应返回一个完整消息");
    assert_eq!(
        second.items[0].text, "second",
        "恢复后应读取第二条连接返回的最新历史"
    );
    assert_ne!(
        second.items[0].text, "first",
        "恢复后不得继续复用第一条失效连接"
    );
    server.join().expect("mock Codex 服务不应失败");
}
