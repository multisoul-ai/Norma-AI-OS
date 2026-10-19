use crate::LiveError;
use reqwest::{
    header::{HeaderValue, AUTHORIZATION},
    Client, Upgraded,
};
use std::time::Duration;
use tokio_tungstenite::{
    tungstenite::{
        handshake::{client::generate_key, derive_accept_key},
        protocol::{Role, WebSocketConfig},
    },
    WebSocketStream,
};

pub(crate) type Socket = WebSocketStream<Upgraded>;
pub(crate) const ENDPOINT: &str = "https://api.openai.com/v1/live/sessions";

pub(crate) async fn connect(
    endpoint: &str,
    api_key: &str,
    loopback: bool,
) -> Result<Socket, LiveError> {
    let mut builder = Client::builder()
        .http1_only()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10));
    if loopback {
        builder = builder.no_proxy();
    }
    // reqwest retains the existing HTTP(S)/SOCKS environment proxy support for the upgrade.
    let client = builder.build().map_err(|_| LiveError::Connection)?;
    let mut authorization = HeaderValue::from_str(&format!("Bearer {}", api_key.trim()))
        .map_err(|_| LiveError::Connection)?;
    authorization.set_sensitive(true);
    let key = generate_key();
    let response = client
        .get(endpoint)
        .header(AUTHORIZATION, authorization)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", &key)
        .send()
        .await
        .map_err(|_| LiveError::Connection)?;
    if matches!(response.status().as_u16(), 401 | 403) {
        return Err(LiveError::Authentication(response.status().as_u16()));
    }
    let headers = response.headers();
    let accept = derive_accept_key(key.as_bytes());
    let valid_connection = headers
        .get("connection")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',')
                .any(|t| t.trim().eq_ignore_ascii_case("upgrade"))
        });
    if response.status().as_u16() != 101
        || !valid_connection
        || !headers
            .get("upgrade")
            .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"websocket"))
        || headers
            .get("sec-websocket-accept")
            .is_none_or(|v| v.as_bytes() != accept.as_bytes())
        || headers.contains_key("sec-websocket-extensions")
        || headers.contains_key("sec-websocket-protocol")
    {
        return Err(LiveError::Connection);
    }
    let stream = response
        .upgrade()
        .await
        .map_err(|_| LiveError::Connection)?;
    let config = WebSocketConfig::default()
        .max_message_size(Some(128 * 1024))
        .max_frame_size(Some(128 * 1024))
        .write_buffer_size(0)
        .max_write_buffer_size(256 * 1024);
    Ok(WebSocketStream::from_raw_socket(stream, Role::Client, Some(config)).await)
}
