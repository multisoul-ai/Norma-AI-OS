use norma::agent_hook::{
    forward_native_question, AgentHookEvent, HookForwardEnv, HookQuestionRequest,
    NativeQuestionReply,
};
use norma::protocol::{AgentProfile, QuestionAnswer};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::time::Duration;

/// 场景：Codex relay通过鉴权本地桥取宿主授权的规范化答案，任意RPC/配置字段不能进入执行。
/// 构造：真实来源与pending、一次200回复；另两轮是无效选项与204取消。
/// 执行：本地服务器核对原run/connection/RPC后回包，bridge校验完整答案并交还relay。
/// 预期：合法Green可返回；服务器塞入新RPC方法无效、无效选项拒绝、取消不返回答案。
#[test]
fn native_bridge_returns_only_validated_answers_for_original_question() {
    for mode in ["answer", "invalid", "cancel", "missing-id"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let env = HookForwardEnv {
            node_id: 7,
            endpoint: format!("http://{}/agent-hooks", listener.local_addr().unwrap()),
            token: "private-token".into(),
        };
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert_eq!(
                line, "POST /agent-hooks/question HTTP/1.1\r\n",
                "只能访问原问答桥"
            );
            let mut size = None;
            let mut authorized = false;
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(length) = line.strip_prefix("Content-Length: ") {
                    size = Some(length.trim().parse::<usize>().unwrap());
                }
                authorized |= line == "Authorization: Bearer private-token\r\n";
            }
            assert!(authorized, "沿用宿主私有鉴权");
            let mut body = vec![0; size.unwrap()];
            reader.read_exact(&mut body).unwrap();
            let request: HookQuestionRequest = serde_json::from_slice(&body).unwrap();
            assert_eq!(request.run_id, "owned-run", "绑定原进程");
            assert_eq!(request.event.raw_json["request"]["id"], 0, "保持数值RPC ID");
            let body = if mode == "cancel" {
                String::new()
            } else {
                json!({
                "delivery_id": if mode == "missing-id" {""} else {"delivery"},
                "answers":[{"key":"question-0","selected":[if mode == "answer" {"option-1"} else {"nonexistent"}]}],
                "method":"turn/start", "params":{"input":"must not execute"}
            }).to_string()
            };
            write!(
                stream,
                "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                if mode == "cancel" {
                    "204 No Content"
                } else {
                    "200 OK"
                },
                body.len(),
                body
            )
            .unwrap();
        });
        let request: Value =
            serde_json::from_str(include_str!("fixtures/questions/codex-request.json")).unwrap();
        let source: Value =
            serde_json::from_str(include_str!("fixtures/questions/codex-source.json")).unwrap();
        let event = AgentHookEvent {
            control_run_id: Some("owned-run".into()),
            node_id: 7,
            profile: AgentProfile::Codex,
            provider_event: "NormaQuestion".into(),
            session_id: Some("codex-question-test-thread".into()),
            turn_id: Some("codex-question-test-turn".into()),
            tool_name: Some("request_user_input".into()),
            message: None,
            raw_json: json!({"connection_id":"codex-relay:instance:1","request":request,"source":source}),
        };
        let result = forward_native_question(
            HookQuestionRequest {
                run_id: "owned-run".into(),
                event,
            },
            &env,
        );
        match mode {
            "answer" => assert_eq!(
                result.unwrap(),
                Some(NativeQuestionReply {
                    delivery_id: "delivery".into(),
                    answers: vec![QuestionAnswer {
                        key: "question-0".into(),
                        selected: vec!["option-1".into()],
                        text: None
                    }]
                }),
                "仅返回已校验答案，不返回任意命令字段"
            ),
            "cancel" => assert_eq!(result.unwrap(), None, "取消等待不伪造空答案"),
            _ => assert!(result.is_err(), "未知选项不能进入relay发送"),
        }
        server.join().unwrap();
    }
}
