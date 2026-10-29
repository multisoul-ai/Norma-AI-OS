use norma::opencode::client::{OpenCodeQuestionClient, ReplyError};
use norma::protocol::QuestionAnswer;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::time::Duration;

fn request() -> Value {
    serde_json::from_str(include_str!("fixtures/questions/opencode-request.json")).unwrap()
}
fn answers() -> Vec<QuestionAnswer> {
    vec![
        QuestionAnswer {
            key: "question-0".into(),
            selected: vec!["option-1".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-1".into(),
            selected: vec!["option-0".into(), "option-2".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-2".into(),
            selected: vec![],
            text: Some("marker".into()),
        },
    ]
}
fn server(reply_status: u16) -> (String, std::thread::JoinHandle<Vec<(String, Value)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let worker = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for index in 0..3 {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("应收到有界本机HTTP请求：{error}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut length = 0;
            let mut authorized = false;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
                if lower.starts_with("authorization:") {
                    authorized = line.contains("Basic b3BlbmNvZGU6dGVzdC1zZWNyZXQ=");
                }
            }
            assert!(authorized, "所有请求都必须使用本实例鉴权");
            assert!(length < 8192, "测试请求体必须有界");
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            requests.push((first, serde_json::from_slice(&body).unwrap_or(Value::Null)));
            let (status, body) = if index < 2 {
                let mut other = request();
                other["sessionID"] = "foreign-session".into();
                (200, json!([other, request()]).to_string())
            } else {
                (
                    reply_status,
                    if reply_status == 200 {
                        "true".into()
                    } else {
                        "{}".into()
                    },
                )
            };
            write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        requests
    });
    (endpoint, worker)
}

/// 场景：同session/request跨instance不可提交，每次HTTP仍绑定原目录及鉴权。
/// 构造：本机测试server同时返回其它session和原题，另建相同URL但不同连接代次的client。
/// 执行：读取pending、用另一连接提交（应无IO）、原连接重新核验并提交；分别返回200和404。
/// 预期：只暴露原session，错代次拒绝；200只提交，404不重试；题序、目录、原request路径精确。
#[test]
fn opencode_replies_only_through_own_instance_and_original_request() {
    for status in [200, 404] {
        let (endpoint, worker) = server(status);
        let raw = request();
        let session = raw["sessionID"].as_str().unwrap();
        let client = OpenCodeQuestionClient::new(
            &endpoint,
            "/tmp/original cwd",
            session,
            "opencode",
            "test-secret",
        )
        .unwrap();
        let other = OpenCodeQuestionClient::new(
            &endpoint,
            "/tmp/original cwd",
            session,
            "opencode",
            "test-secret",
        )
        .unwrap();
        let pending = client.pending().unwrap();
        assert_eq!(pending.len(), 1, "GET结果必须先按所属session筛选");
        assert_eq!(
            other.reply(&pending[0], &answers()),
            Err(ReplyError::WrongConnection),
            "同session ID也不能跨连接代次发送"
        );
        let result = client.reply(&pending[0], &answers());
        assert_eq!(
            result,
            if status == 200 {
                Ok(())
            } else {
                Err(ReplyError::NotPending)
            },
            "只有原HTTP成功可记提交，404不当作成功"
        );
        let requests = worker.join().unwrap();
        assert_eq!(
            requests.len(),
            3,
            "首次读取、发送前核验、唯一POST；不得盲目重试"
        );
        assert!(
            requests
                .iter()
                .all(|(line, _)| line.contains("directory=%2Ftmp%2Foriginal+cwd")),
            "每次请求都绑定原目录"
        );
        assert!(
            requests[2].0.starts_with(&format!(
                "POST /question/{}/reply?",
                raw["id"].as_str().unwrap()
            )),
            "只提交原request ID"
        );
        assert_eq!(
            requests[2].1,
            json!({"answers":[["Green"],["macOS","Linux"],["marker"]]}),
            "实际HTTP请求按原题顺序回填"
        );
    }
}
