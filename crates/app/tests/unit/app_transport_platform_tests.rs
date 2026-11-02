use crate::transport::NormaSocketListener;
use norma::client::{decode_response_line, encode_request_line};
use norma::codex::UnixStream;
use norma::protocol::{
    CardCommand, CardCommandResult, CardKind, Placement, WireRequest, WireResponse,
};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn endpoint() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::Builder::new()
        .prefix("nat-")
        .tempdir_in(if cfg!(windows) {
            std::env::temp_dir()
        } else {
            "/tmp".into()
        })
        .unwrap();
    let path = dir.path().join("s.sock");
    (dir, path)
}
fn request() -> WireRequest {
    WireRequest {
        protocol_version: norma::protocol::CURRENT_PROTOCOL_VERSION,
        client_version: None,
        request_id: 17,
        operation_id: None,
        authorization: None,
        source_workspace_id: None,
        source_workspace_route_id: None,
        target_workspace_id: None,
        source_node_id: None,
        command: CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        },
    }
}
fn reply(listener: &NormaSocketListener) {
    let deadline = Instant::now() + Duration::from_secs(2);
    let incoming = loop {
        if let Some(request) = listener.try_recv() {
            break request;
        }
        assert!(
            Instant::now() < deadline,
            "完整NDJSON请求必须到达原有有界队列"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(
        incoming.request,
        request(),
        "原请求身份与命令不能在传输边界改变"
    );
    incoming
        .response
        .send(WireResponse::new(
            17,
            Ok(CardCommandResult::Created { node_id: 4 }),
        ))
        .unwrap();
}
fn response(stream: UnixStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    let response = decode_response_line(&line).unwrap();
    assert_eq!(response.request_id, 17, "响应只能交回原request_id");
    assert_eq!(
        response.result,
        Ok(CardCommandResult::Created { node_id: 4 }),
        "传输不能伪造执行结果"
    );
}

/// 场景：后台accept得到的非阻塞连接也必须完整读取分段NDJSON请求。
/// 构造：真实App listener与客户端，合法请求前2字节和余下正文间隔50ms；50ms大于10ms accept轮询，远小于2s流超时。
/// 执行：分两次写入同一请求，处理App队列，再读取原客户端响应。
/// 预期：只接受完整请求，保留request_id/命令/结果，不把临时WouldBlock当作坏请求。
#[test]
fn segmented_request_keeps_original_identity_and_response() {
    let (_dir, path) = endpoint();
    let listener = NormaSocketListener::bind(path).unwrap();
    let mut client = UnixStream::connect(_dir.path().join("s.sock")).unwrap();
    let wire = encode_request_line(&request()).unwrap();
    client.write_all(&wire.as_bytes()[..2]).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    client
        .write_all(&wire.as_bytes()[2..])
        .expect("服务端不能在完整请求到达前提前关闭连接");
    reply(&listener);
    response(client);
}

/// 场景：同一端点的重复绑定不能夺走租约或删除正在服务的socket。
/// 构造：一个真实listener和相同路径的第二次bind；请求固定ID17。
/// 执行：拒绝第二次bind后，通过原端点完成请求响应，再正常关闭。
/// 预期：第二次得到AddrInUse，原端点仍可用，最终只清理自己拥有的socket。
#[test]
fn duplicate_binding_preserves_live_endpoint_and_cleans_up() {
    let (_dir, path) = endpoint();
    let listener = NormaSocketListener::bind(path.clone()).unwrap();
    let error = NormaSocketListener::bind(path.clone())
        .err()
        .expect("同端点重复绑定必须失败");
    assert_eq!(
        error.kind(),
        std::io::ErrorKind::AddrInUse,
        "租约冲突必须有明确错误"
    );
    let mut client = UnixStream::connect(&path).unwrap();
    client
        .write_all(encode_request_line(&request()).unwrap().as_bytes())
        .unwrap();
    reply(&listener);
    response(client);
    drop(listener);
    assert!(!path.exists(), "正常关闭必须清理自己拥有的socket");
}

/// 场景：端点位置被普通文件占据时，服务端必须拒绝且不删除用户正文。
/// 构造：在预定socket路径写入固定标记。
/// 执行：尝试bind并读取原文件。
/// 预期：AddrInUse且正文逐字节不变，不能把普通文件作为陈旧端点清理。
#[test]
fn occupied_regular_file_is_never_removed() {
    let (_dir, path) = endpoint();
    std::fs::write(&path, b"keep-user-data").unwrap();
    let error = NormaSocketListener::bind(path.clone())
        .err()
        .expect("普通文件必须阻止绑定");
    assert_eq!(
        error.kind(),
        std::io::ErrorKind::AddrInUse,
        "非socket占用必须明确拒绝"
    );
    assert_eq!(
        std::fs::read(path).unwrap(),
        b"keep-user-data",
        "禁止删除或截断原正文"
    );
}
