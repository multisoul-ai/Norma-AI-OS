use super::*;
use norma::agent_hook::{parse_hook_event, HookQuestionRequest, HookSource};
use norma::protocol::QuestionAnswer;
use std::io::Read;
use std::net::TcpListener;

/// 场景：HTTP响应已有字节写出后工作区被撤销，剩余答案不能继续发出。
/// 构造：本地真实socket、一张Ready问题票据；首字节手动写入模拟部分写成功。
/// 执行：撤销路由，继续轮询原waiter，然后从客户端读取连接最终内容。
/// 预期：立即结束且仅收到已写首字节，不补全答案，也不追加另一段204响应。
#[test]
fn revoked_partial_question_response_closes_without_more_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .unwrap();
    let (server, _) = listener.accept().unwrap();
    let registry = Arc::new(Mutex::new(AgentHookRegistry::default()));
    let (token, ..) = registry.lock().unwrap().register().unwrap();
    let raw = serde_json::from_str(include_str!(
        "../../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
    ))
    .unwrap();
    let event = parse_hook_event(HookSource::Claude, 1, raw).unwrap();
    let ticket = HookQuestion::new(
        HookQuestionRequest {
            run_id: "run".into(),
            event,
        },
        Instant::now(),
    )
    .unwrap();
    ticket
        .submit_answer(
            &[QuestionAnswer {
                key: "question-0".into(),
                selected: vec!["option-1".into()],
                text: None,
            }],
            Instant::now(),
        )
        .unwrap();
    let response = ticket.take_response(Instant::now()).unwrap();
    let mut waiter = WaitingHook::new_question(server, token.clone(), ticket.clone()).unwrap();
    waiter.output = http::response("200 OK", response.to_string().as_bytes());
    waiter.stream.write_all(&waiter.output[..1]).unwrap();
    waiter.written = 1;
    registry.lock().unwrap().unregister(&token);
    assert!(!waiter.poll(&registry), "部分写入后撤销必须关闭连接");
    drop(waiter);
    let mut bytes = Vec::new();
    client.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"H", "不得追加旧答案或新HTTP响应");
    assert_eq!(
        ticket.status(Instant::now()),
        QuestionHookStatus::Cancelled,
        "未完整响应的句柄仍可撤销"
    );
}
