use norma::client::{decode_request_line, encode_response_line, send_command_to_path, ClientError};
use norma::protocol::{CardCommand, CardCommandResult, WireResponse, CURRENT_PROTOCOL_VERSION};
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::UnixListener;
#[cfg(windows)]
use uds_windows::UnixListener;

fn exchange(wrong_id: bool) -> Result<CardCommandResult, ClientError> {
    let directory = tempfile::tempdir().expect("创建独立 socket 目录");
    let path = directory.path().join("client.sock");
    let listener = UnixListener::bind(&path).expect("绑定真实本地 socket");
    listener
        .set_nonblocking(true)
        .expect("测试 accept 必须有界");
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline, "客户端应在五秒内连接");
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => panic!("接受本地连接：{error}"),
            }
        };
        stream
            .set_nonblocking(false)
            .expect("Windows accept 后显式恢复阻塞读取");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("限制测试读取时间");
        let mut line = String::new();
        BufReader::new(&mut stream)
            .read_line(&mut line)
            .expect("读取 NDJSON 请求");
        let request = decode_request_line(&line).expect("解码公共协议");
        assert_eq!(request.command, CardCommand::List, "传输不得改变命令");
        let response = WireResponse {
            server_version: env!("CARGO_PKG_VERSION").into(),
            protocol_version: CURRENT_PROTOCOL_VERSION,
            request_id: if wrong_id {
                request.request_id.wrapping_add(1)
            } else {
                request.request_id
            },
            result: Ok(CardCommandResult::Listed { cards: Vec::new() }),
        };
        stream
            .write_all(
                encode_response_line(&response)
                    .expect("编码应答")
                    .as_bytes(),
            )
            .expect("发送应答");
    });
    let result = send_command_to_path(&path, CardCommand::List);
    server.join().expect("协议服务线程必须正常完成");
    result
}

/// 场景：真实本地 Socket 上发送 List；构造隔离 listener 和同请求 ID 应答；
/// 执行公共 client API；预期得到 Listed，不能被误解为空响应或其他命令结果。
#[test]
fn local_socket_client_round_trips_the_existing_wire_protocol() {
    let result = exchange(false).expect("Unix 与 Windows AF_UNIX 均应传递原有协议");
    assert_eq!(
        result,
        CardCommandResult::Listed { cards: Vec::new() },
        "应返回服务端的准确结果"
    );
}

/// 场景：本地服务返回错误关联 ID；构造请求 ID+1 的有效协议响应；
/// 执行公共 client API；预期具体的 MismatchedResponseId，不能接受为成功。
#[test]
fn local_socket_client_rejects_a_response_for_another_request() {
    let result = exchange(true);
    assert!(
        matches!(result, Err(ClientError::MismatchedResponseId { .. })),
        "不属于原请求的应答必须拒绝：{result:?}"
    );
    assert!(result.is_err(), "错误关联 ID 不得被当作成功");
}
