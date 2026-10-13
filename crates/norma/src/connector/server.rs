//! Loopback only. The owning App drops this handle on quit; there is no daemon lifecycle.
use super::{Auth, ConnectorConfig, ConnectorTools};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use hyper::server::conn::http1;
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::Semaphore,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;
const BODY_LIMIT: usize = 64 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_CONNECTIONS: usize = 8;

pub struct ConnectorServer {
    address: SocketAddr,
    stop: CancellationToken,
    task: Option<JoinHandle<()>>,
}
impl ConnectorServer {
    /// Disabled configurations return without binding, reading secrets or creating a listener.
    pub async fn start(
        config: ConnectorConfig,
        auth: Arc<Auth>,
        tools: ConnectorTools,
    ) -> std::io::Result<Option<Self>> {
        if !config.enabled {
            return Ok(None);
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, config.port)).await?;
        let address = listener.local_addr()?;
        let stop = CancellationToken::new();
        let shutdown = stop.clone();
        let task = tokio::spawn(async move {
            serve(listener, address.port(), auth, tools, shutdown).await;
        });
        Ok(Some(Self {
            address,
            stop,
            task: Some(task),
        }))
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub async fn stop(mut self) {
        self.stop.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}
impl Drop for ConnectorServer {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
#[derive(Clone)]
struct Gate {
    auth: Arc<Auth>,
    port: u16,
    // P3 can explicitly supply an authority here. No public wildcard or tunnel setup in P2.
    tunnel_authority: Option<String>,
}
impl Gate {
    fn authorities(&self) -> Vec<String> {
        let mut hosts = vec![
            format!("127.0.0.1:{}", self.port),
            format!("localhost:{}", self.port),
        ];
        if let Some(tunnel) = &self.tunnel_authority {
            hosts.push(tunnel.clone());
        }
        hosts
    }
    fn allows_host(&self, value: &str) -> bool {
        self.authorities().iter().any(|host| host == value)
    }
}
fn router(gate: Gate, tools: ConnectorTools, sessions: CancellationToken) -> Router {
    let config = StreamableHttpServerConfig::default()
        .with_cancellation_token(sessions)
        .with_max_request_body_bytes(BODY_LIMIT)
        .with_allowed_hosts(gate.authorities())
        .enforce_origin_validation();
    let service = StreamableHttpService::new(
        move || Ok(tools.clone()),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    Router::new()
        .route_service("/mcp", service)
        .layer(middleware::from_fn_with_state(gate, authorize))
}
async fn authorize(
    axum::extract::State(gate): axum::extract::State<Gate>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if request.uri().path() != "/mcp" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let hosts = request.headers().get_all("host");
    if hosts.iter().count() != 1
        || hosts
            .iter()
            .next()
            .and_then(|v| v.to_str().ok())
            .is_none_or(|h| !gate.allows_host(h))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned);
    let auth = gate.auth.clone();
    let identity = tokio::task::spawn_blocking(move || {
        let result = auth.authenticate(token.as_deref());
        if result.is_err() {
            auth.audit("authentication", "unauthorized")
                .map_err(|_| "internal")?;
        }
        result
    })
    .await;
    let identity = match identity {
        Ok(Ok(identity)) => identity,
        Ok(Err("unauthorized")) => return (StatusCode::UNAUTHORIZED, axum::Json(serde_json::json!({"error":{"code":"unauthorized","message":"连接令牌无效","retryable":false}}))).into_response(),
        _ => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let sessions = gate.auth.sessions();
    let (mut parts, body) = request.into_parts();
    parts.headers.remove("authorization");
    parts.extensions.insert(identity);
    let bytes = match tokio::time::timeout(READ_TIMEOUT, to_bytes(body, BODY_LIMIT)).await {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        Err(_) => return StatusCode::REQUEST_TIMEOUT.into_response(),
    };
    tokio::select! {
        biased;
        _ = sessions.cancelled() => StatusCode::UNAUTHORIZED.into_response(),
        response = next.run(Request::from_parts(parts, Body::from(bytes))) => response,
    }
}
async fn serve(
    listener: TcpListener,
    port: u16,
    auth: Arc<Auth>,
    tools: ConnectorTools,
    stop: CancellationToken,
) {
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    loop {
        let sessions = auth.sessions().child_token();
        let _session_guard = sessions.clone().drop_guard();
        let router = router(
            Gate {
                auth: auth.clone(),
                port,
                tunnel_authority: None,
            },
            tools.clone(),
            sessions.clone(),
        );
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = stop.cancelled() => { sessions.cancel(); return; }
                _ = sessions.cancelled() => break,
                Some(_) = connections.join_next(), if !connections.is_empty() => {},
                accepted = async {
                    let permit = slots.clone().acquire_owned().await.expect("connection slots");
                    listener.accept().await.map(|(stream,peer)|(stream,peer,permit))
                } => {
                    let Ok((stream, peer, permit)) = accepted else { sessions.cancel(); return; };
                    if !peer.ip().is_loopback() { continue; }
                    let router = router.clone();
                    let cancellation = sessions.clone();
                    connections.spawn(async move {
                        let _permit = permit;
                        let connection = http1::Builder::new().timer(TokioTimer::new()).header_read_timeout(READ_TIMEOUT).serve_connection(TokioIo::new(stream), TowerToHyperService::new(router));
                        tokio::select! { _ = cancellation.cancelled() => {}, _ = connection => {} }
                    });
                }
            }
        }
        // Cancelling rmcp plus dropping JoinSet aborts both SSE and keep-alive TCP connections.
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
}
