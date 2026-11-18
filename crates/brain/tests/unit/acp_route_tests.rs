use super::{session_id, Switchboard};
use crate::acp::wire::Incoming;
use serde_json::json;

/// 场景：会话 B 的权限请求不能进会话 A 的信箱。
/// 数据构造：两个已登记会话；一条带 sessionId=B 的 session/request_permission。
/// 执行过程：把这条请求送进交换机，再从两边的信箱各读一次。
/// 预期结果：B 收到这条权限请求；A 的信箱是空的。
#[test]
fn permission_for_b_does_not_enter_a() {
    let mut board = Switchboard::new();
    let a = board.attach("sess-a");
    let b = board.attach("sess-b");
    let delivered = board.deliver(Incoming::Request {
        id: json!(7),
        method: "session/request_permission".into(),
        params: json!({"sessionId": "sess-b", "toolCall": {"name": "Bash"}}),
    });
    assert!(delivered, "带 sessionId 的权限请求必须送到对应会话");
    let got = b
        .recv_timeout(std::time::Duration::from_millis(50))
        .expect("B 应该收到自己的权限请求");
    match got {
        Incoming::Request { method, params, .. } => {
            assert_eq!(
                method, "session/request_permission",
                "送到 B 的必须是权限请求"
            );
            assert_eq!(
                session_id(&params).as_deref(),
                Some("sess-b"),
                "权限请求仍带着 B 的 sessionId"
            );
        }
        other => panic!("B 收到的不是请求: {other:?}"),
    }
    assert!(
        a.recv_timeout(std::time::Duration::from_millis(20))
            .is_err(),
        "A 的信箱不能出现 B 的权限请求"
    );
}

/// 场景：响应只回到登记过这个请求 id 的会话。
/// 数据构造：请求 4 绑在 A，请求 5 绑在 B；先回 5 再回 4。
/// 执行过程：按 id 投递两条响应。
/// 预期结果：B 只拿到 5，A 只拿到 4。
#[test]
fn responses_follow_the_request_id() {
    let mut board = Switchboard::new();
    let a = board.attach("sess-a");
    let b = board.attach("sess-b");
    board.bind(4, "sess-a");
    board.bind(5, "sess-b");
    assert!(
        board.deliver(Incoming::Response {
            id: 5,
            result: Ok(json!({"stopReason": "end_turn"})),
        }),
        "已登记的响应必须送到 B"
    );
    assert!(
        board.deliver(Incoming::Response {
            id: 4,
            result: Ok(json!({"stopReason": "end_turn"})),
        }),
        "已登记的响应必须送到 A"
    );
    match b.recv_timeout(std::time::Duration::from_millis(50)) {
        Ok(Incoming::Response { id, .. }) => assert_eq!(id, 5, "B 只能拿到自己的请求 5"),
        other => panic!("B 没有拿到响应: {other:?}"),
    }
    match a.recv_timeout(std::time::Duration::from_millis(50)) {
        Ok(Incoming::Response { id, .. }) => assert_eq!(id, 4, "A 只能拿到自己的请求 4"),
        other => panic!("A 没有拿到响应: {other:?}"),
    }
}
