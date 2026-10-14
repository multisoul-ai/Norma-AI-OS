use norma::connector::*;
use serde_json::json;
use std::sync::{Arc, Mutex};
// Keep this integration test's adapter local: other connector tests use the in-memory seam.
#[path = "support/sqlite.rs"]
mod sqlite;
struct Empty;
impl DataSource for Empty {
    fn snapshot(&self) -> anyhow::Result<Snapshot> {
        Ok(Snapshot::default())
    }
}
#[derive(Clone)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for LogBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("日志缓冲").extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// 场景：在临时SQLite生成两代随机令牌和密钥，重开后查看/鉴权/签名，然后执行HTTP错误与吊销。
/// 执行：捕获TRACE级tracing，真实HTTP携带随机令牌与未知参数；读取原始审计字段再重开库。
/// 预期：当前/宽限令牌和签名重启稳定；日志、审计、错误、Debug均无秘密，吊销删除明文并断开会话。
#[tokio::test]
async fn sqlite_restart_display_and_secret_free_diagnostics() {
    let logs = LogBuffer(Arc::new(Mutex::new(Vec::new())));
    let writer = logs.clone();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish(),
    )
    .expect("隔离测试日志订阅");
    let dir = tempfile::tempdir().expect("隔离目录");
    let path = dir.path().join("app.sqlite");
    let store = storage::connector_store::ConnectorStore::new(path.clone());
    let auth = Auth::load(
        Arc::new(sqlite::SqliteCredentials(store.clone())),
        Arc::new(SystemClock),
    )
    .expect("加载");
    let old = auth.generate().expect("旧令牌");
    let current = auth.generate().expect("当前令牌");
    let card_key = auth.signer_key().expect("身份密钥");
    let webhook_key = auth.webhook_key().expect("HMAC密钥");
    assert!(card_key != webhook_key, "独立密钥");
    let identity = CardIdentity {
        workspace_id: "w".into(),
        area_id: "a".into(),
        canvas_id: "c".into(),
        node_id: 1,
        definition_id: 1,
        created_at: 100,
    };
    let card_id = CardSigner::new(&card_key).sign(&identity);
    drop(auth);
    drop(store);
    let reopened = storage::connector_store::ConnectorStore::new(path.clone());
    let auth = Arc::new(
        Auth::load(
            Arc::new(sqlite::SqliteCredentials(reopened.clone())),
            Arc::new(SystemClock),
        )
        .expect("重开DB加载"),
    );
    let visible = auth.visible_tokens().expect("显示");
    assert_eq!(visible.len(), 2, "当前和宽限令牌均可显示");
    assert!(
        visible
            .iter()
            .any(|t| t.token == current && t.expires_at.is_none()),
        "当前明文重开可读"
    );
    assert!(
        visible
            .iter()
            .any(|t| t.token == old && t.expires_at.is_some()),
        "旧明文重开可读"
    );
    assert!(
        auth.webhook_key().expect("HMAC重开") == webhook_key,
        "HMAC稳定"
    );
    let signer = CardSigner::new(&auth.signer_key().expect("card密钥重开"));
    assert_eq!(
        signer.verify(&card_id).expect("旧签名"),
        identity,
        "card_id重开保持有效"
    );
    let tools = ConnectorTools::new(auth.clone(), signer, Arc::new(Empty));
    let authorized = auth.authenticate(Some(&current)).expect("当前授权");
    assert!(auth.authenticate(Some(&old)).is_ok(), "旧令牌宽限授权");
    // Pass secrets as unknown fields: diagnostics must not echo user input or internal errors.
    let tool_error =
        serde_json::to_string(&tools.call(&authorized, "directory", json!({"unexpected":current})))
            .expect("错误序列化");
    assert!(
        tool_error.contains("invalid_argument"),
        "未知参数按结构化错误处理"
    );
    tracing::debug!(credentials = ?visible, "connector credential metadata");
    let server = ConnectorServer::start(
        ConnectorConfig {
            enabled: true,
            port: 0,
            tunnel_authority: None,
        },
        auth.clone(),
        tools,
    )
    .await
    .expect("监听")
    .expect("开启");
    let client = reqwest::Client::new();
    let url = format!("http://{}/mcp", server.address());
    let init = client.post(&url).bearer_auth(&current).header("accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"secret-test","version":"1"}}}))
        .send().await.expect("初始化HTTP");
    assert_eq!(init.status(), 200, "真实MCP授权可用");
    let _ = init.text().await.expect("初始化响应");
    let session = auth.sessions();
    auth.revoke().expect("吊销");
    assert!(session.is_cancelled(), "立即取消所有旧会话");
    let response = reqwest::Client::new()
        .post(&url)
        .bearer_auth(&current)
        .json(&json!({}))
        .send()
        .await
        .expect("吊销HTTP");
    assert_eq!(response.status(), 401, "吊销令牌立即401");
    let error = response.text().await.expect("错误响应");
    server.stop().await;
    assert!(
        storage::connector_store::ConnectorStore::new(path.clone())
            .tokens()
            .expect("再次重开")
            .is_empty(),
        "SQLite明文行删除"
    );
    let conn = rusqlite_connection(&path);
    let mut stmt = conn.prepare("SELECT tool,parameter_summary,result_code,coalesce(user_quote,''),coalesce(operation_id,'') FROM connector_audit").expect("原始审计");
    let audit = stmt
        .query_map([], |r| {
            Ok(format!(
                "{} {} {} {} {}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?
            ))
        })
        .expect("审计行")
        .collect::<Result<Vec<_>, _>>()
        .expect("审计字段")
        .join("\n");
    let tracing_output = String::from_utf8(logs.0.lock().expect("捕获").clone()).expect("UTF8日志");
    assert!(
        tracing_output.contains("connector credential metadata"),
        "捕获确实包含测试tracing事件"
    );
    assert!(
        audit.contains("invalid_argument") && audit.contains("unauthorized"),
        "审计确实覆盖工具与鉴权错误"
    );
    for secret in [
        &current,
        &old,
        &hex::encode(&card_key),
        &hex::encode(&webhook_key),
    ] {
        assert!(!tracing_output.contains(secret), "tracing不得泄漏秘密");
        assert!(!audit.contains(secret), "原始审计不得泄漏秘密");
        assert!(
            !error.contains(secret) && !tool_error.contains(secret),
            "错误不得泄漏秘密"
        );
        assert!(
            !format!("{visible:?}").contains(secret),
            "Debug不得泄漏秘密"
        );
    }
}
fn rusqlite_connection(path: &std::path::Path) -> rusqlite::Connection {
    rusqlite::Connection::open(path).expect("临时库只读检查")
}
