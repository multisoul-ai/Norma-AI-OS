use crate::jev_choice::{JevChoiceClient, JevChoiceRequest, JevError};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// 场景：旁路请求在发送前已取消，不应建立任何网络请求。
/// 构造：仅使用假的测试凭据和人工文本，取消 token 先置位。
/// 执行：公开 observe 接口。
/// 预期：返回固定 Cancelled，不把凭据放进 Debug 或错误文本。
#[tokio::test]
async fn cancelled_observation_never_starts_a_request() {
    let client = JevChoiceClient::new("fixture-private-key".into()).expect("隔离客户端");
    let request = JevChoiceRequest::new(
        "人工合成请求".into(),
        BTreeMap::from([
            ("PLAN_0".into(), "Henry 检查方案".into()),
            ("CLARIFY".into(), "仅补问目标".into()),
        ]),
    )
    .expect("合成请求");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = client
        .observe(&request, Duration::from_secs(1), cancel)
        .await;
    assert_eq!(
        result,
        Err(JevError::Cancelled),
        "已取消请求必须在网络前退出"
    );
    assert!(
        !format!("{result:?}").contains("fixture-private-key"),
        "错误不能泄漏凭据"
    );
}

/// 场景：旁路服务迟迟不回或返回 HTTP 错误，不能重试或暴露响应中的凭据。
/// 构造：本机一次性 TCP 服务，第一轮保持静默，第二轮响应 401 和敏感假正文。
/// 执行：公开 observe，以短期限验证超时，再验证固定错误结构。
/// 预期：迟到请求被放弃，错误仅包含状态码，未包含响应正文。
#[tokio::test]
async fn stalled_and_http_error_observations_are_bounded_and_redacted() {
    use std::io::{Read, Write};
    let request = JevChoiceRequest::new(
        "人工合成请求".into(),
        BTreeMap::from([
            ("PLAN_0".into(), "Henry 检查方案".into()),
            ("CLARIFY".into(), "仅补问目标".into()),
        ]),
    )
    .expect("合成请求");
    for stalled in [true, false] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("回环端口");
        let endpoint = format!(
            "http://{}/v1/systemone",
            listener.local_addr().expect("端口")
        );
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("仅接收一次请求");
            let mut bytes = [0; 8192];
            let _ = stream.read(&mut bytes);
            if stalled {
                std::thread::sleep(Duration::from_millis(100));
            } else {
                let body = "fixture-private-response";
                let _=write!(stream,"HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
            }
        });
        let client = JevChoiceClient::for_loopback(endpoint);
        let result = client
            .observe(
                &request,
                if stalled {
                    Duration::from_millis(30)
                } else {
                    Duration::from_secs(1)
                },
                CancellationToken::new(),
            )
            .await;
        assert_eq!(
            result,
            Err(if stalled {
                JevError::Timeout
            } else {
                JevError::Http { status: 401 }
            }),
            "旁路异常应有确定退出结果"
        );
        assert!(
            !format!("{result:?}").contains("fixture-private-response"),
            "服务原始错误正文必须脱敏"
        );
        server.join().expect("服务线程退出");
    }
}

/// 场景：请求已到服务端，随后取消，而服务端继续返回迟到的合法选择。
/// 构造：回环服务发出收到请求的信号，延迟 100ms 后再返回完整分布。
/// 执行：收到信号后取消并等待观察任务；最后等待服务端退出。
/// 预期：客户端只返回 Cancelled，迟到的合法选择没有机会成为观察或执行结果。
#[tokio::test]
async fn cancellation_discards_a_late_valid_choice() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("回环端口");
    let endpoint = format!(
        "http://{}/v1/systemone",
        listener.local_addr().expect("端口")
    );
    let (started, received) = tokio::sync::oneshot::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("接收请求");
        let mut bytes = [0; 8192];
        let _ = stream.read(&mut bytes);
        let _ = started.send(());
        std::thread::sleep(Duration::from_millis(100));
        let body = r#"{"model":"jev-1.14.0","answers":{"plan":{"type":"choice","choice":"PLAN_0","confidence":1.0,"probabilities":{"PLAN_0":1.0,"CLARIFY":0.0}}}}"#;
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    let request = JevChoiceRequest::new(
        "人工合成请求".into(),
        BTreeMap::from([
            ("PLAN_0".into(), "Henry 执行".into()),
            ("CLARIFY".into(), "补问".into()),
        ]),
    )
    .expect("合成请求");
    let client = JevChoiceClient::for_loopback(endpoint);
    let cancel = CancellationToken::new();
    let observer_cancel = cancel.clone();
    let observer = tokio::spawn(async move {
        client
            .observe(&request, Duration::from_secs(1), observer_cancel)
            .await
    });
    tokio::time::timeout(Duration::from_secs(1), received)
        .await
        .expect("请求已发送")
        .expect("服务信号");
    cancel.cancel();
    assert_eq!(
        observer.await.expect("观察任务"),
        Err(JevError::Cancelled),
        "已经取消，不能接受迟到的合法选择"
    );
    server.join().expect("服务结束");
}
