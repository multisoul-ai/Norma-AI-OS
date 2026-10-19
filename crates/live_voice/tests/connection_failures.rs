#![cfg(feature = "test-support")]

use futures::{SinkExt, StreamExt};
use live_voice::{connect_loopback, LiveConfig, LiveError};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::{
    accept_async, accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        http, Message,
    },
};
use tokio_util::sync::CancellationToken;

/// AC12：HTTP认证拒绝必须区别于一般网络断线。
///
/// 数据构造：本地WS握手固定返回401，无真实Key。
/// 执行过程：客户端连接假服务，读取公开错误分类。
/// 预期结果：Authentication(401)，不进入started或音频处理。
#[tokio::test]
async fn authentication_rejection_is_reported_without_key_material() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("应绑定");
    let address = listener.local_addr().expect("应有地址");
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.expect("应接收");
        let result = accept_hdr_async(tcp, |_request: &Request, _response: Response| {
            Err(http::Response::builder()
                .status(401)
                .body(Some("not authorized".into()))
                .expect("拒绝响应有效"))
        })
        .await;
        assert!(result.is_err(), "假上游须拒绝而非建立WS");
    });
    let result = connect_loopback(
        address,
        LiveConfig::default(),
        "fixture-key",
        CancellationToken::new(),
    )
    .await;
    assert!(
        matches!(result, Err(LiveError::Authentication(401))),
        "调用方必须能区分凭据拒绝和网络失败"
    );
    server.await.expect("认证夹具应完成");
}

/// AC01/AC12：WS建立后的配置拒绝不能被忽略并等到超时。
///
/// 数据构造：ready前仅发error，其中原始message含私有测试canary。
/// 执行过程：上游保持WS，客户端在2秒边界内返回配置拒绝。
/// 预期结果：ServerRejected且不传播message，不把认证成功当成会话可用。
#[tokio::test]
async fn startup_error_is_not_mistaken_for_ready_or_a_timeout() {
    tokio::time::timeout(Duration::from_secs(2), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("应绑定");
        let address = listener.local_addr().expect("应有地址");
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("应接收");
            let mut socket = accept_async(tcp).await.expect("WS应建立");
            socket.next().await.expect("应收到start").expect("start应有效");
            socket.send(Message::Text(json!({"type":"error","error":{"code":"model_not_found","message":"private-canary"}}).to_string().into()))
                .await.expect("应发送拒绝");
            let _ = socket.next().await;
        });
        let result = connect_loopback(address, LiveConfig::default(), "fixture-key", CancellationToken::new()).await;
        assert!(matches!(result, Err(LiveError::ServerRejected)), "握手成功后仍须检查ready前的服务错误");
        server.await.expect("配置拒绝夹具应退出");
    }).await.expect("不能等到默认10秒启动超时才处理拒绝");
}

/// AC11：服务器不发closed时有界释放，并保留“最终用量未确认”。
///
/// 数据构造：服务接受close但永不回复closed；关闭等待预算5秒。
/// 执行过程：真实网络确认close已到达后暂停Tokio时钟，自动推进到关闭期限。
/// 预期结果：Timeout且finalized=false，不等待真实5秒、不假报关闭成功。
#[tokio::test]
async fn missing_closed_expires_without_confirming_usage() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("应绑定");
    let address = listener.local_addr().expect("应有地址");
    let (sent_close, received_close) = tokio::sync::oneshot::channel();
    let release = CancellationToken::new();
    let peer_release = release.clone();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.expect("应接收");
        let mut socket = accept_async(tcp).await.expect("WS应建立");
        socket
            .next()
            .await
            .expect("应有start")
            .expect("start应有效");
        socket.send(Message::Text(json!({"type":"session.started","session":{"id":"live_timeout","model":"gpt-live-1"}}).to_string().into()))
            .await.expect("应ready");
        let close = socket
            .next()
            .await
            .expect("应有close")
            .expect("close应有效");
        let event: Value =
            serde_json::from_str(close.to_text().expect("close应为文本")).expect("close应为JSON");
        assert_eq!(event["type"], "session.close", "应先尝试优雅关闭");
        sent_close.send(()).expect("测试接收方仍在");
        peer_release.cancelled().await;
    });
    let client = connect_loopback(
        address,
        LiveConfig::default(),
        "fixture-key",
        CancellationToken::new(),
    )
    .await
    .expect("应ready");
    client.control().close();
    tokio::time::timeout(Duration::from_secs(2), received_close)
        .await
        .expect("close应及时发送")
        .expect("假上游收到close");
    tokio::time::pause();
    let outcome = client.finish().await;
    assert_eq!(
        outcome.error,
        Some(LiveError::Timeout),
        "没有终态必须报关闭超时"
    );
    assert!(!outcome.usage.finalized, "没有closed不能确认最终用量");
    release.cancel();
    server.await.expect("假上游应收口");
}
