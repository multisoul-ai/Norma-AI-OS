use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::{net::TcpStream, sync::Arc, time::Duration};
use tungstenite::{Message, WebSocket};

pub(crate) fn connect(
    address: std::net::SocketAddr,
    cert: Vec<u8>,
) -> WebSocket<StreamOwned<ClientConnection, TcpStream>> {
    let mut roots = RootCertStore::empty();
    roots.add(cert.into()).expect("只信任 QR 对应证书");
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .expect("TLS13")
            .with_root_certificates(roots)
            .with_no_client_auth();
    let socket = TcpStream::connect(address).expect("连接");
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("有界读");
    socket
        .set_write_timeout(Some(Duration::from_secs(3)))
        .expect("有界写");
    let tls = StreamOwned::new(
        ClientConnection::new(
            Arc::new(config),
            ServerName::try_from("norma.local").expect("证书名"),
        )
        .expect("TLS"),
        socket,
    );
    tungstenite::client(format!("wss://{address}/mobile"), tls)
        .expect("WSS")
        .0
}

pub(crate) fn exchange(
    socket: &mut WebSocket<StreamOwned<ClientConnection, TcpStream>>,
    machine: &str,
    id: &str,
    action: serde_json::Value,
) -> serde_json::Value {
    socket
        .send(Message::text(
            serde_json::json!({"version":1,"request_id":id,"machine_id":machine,"action":action})
                .to_string(),
        ))
        .expect("发送");
    serde_json::from_str(socket.read().expect("响应").to_text().expect("text")).expect("JSON")
}
