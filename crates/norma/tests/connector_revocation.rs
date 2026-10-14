mod support;
use norma::connector::*;
use serde_json::json;
use std::sync::{Arc, Mutex};
struct Source {
    entered: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}
impl DataSource for Source {
    fn snapshot(&self) -> anyhow::Result<Snapshot> {
        Ok(Snapshot::default())
    }
    fn write(
        &self,
        _epoch: &str,
        _request: WriteRequest,
        _now: i64,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<serde_json::Value, &'static str> {
        self.entered.send(()).expect("通知已进入后台");
        self.release
            .lock()
            .expect("等待锁")
            .recv()
            .expect("等待吊销");
        assert!(cancellation.is_cancelled(), "后台写请求持有原授权的取消域");
        Err("outcome_unknown")
    }
}
/// 场景：写调用已鉴权但尚未执行副作用时吊销；所有数据为随机测试Token。
/// 执行：阻塞在后台执行边界、吊销、释放；预期：原取消域失效，最终响应unauthorized。
#[test]
fn connector_inflight_write_is_bound_to_original_revocation_scope() {
    let auth = Arc::new(
        Auth::load(
            Arc::new(support::Credentials::default()),
            Arc::new(SystemClock),
        )
        .expect("鉴权"),
    );
    let token = auth.generate().expect("随机Token");
    let identity = auth.authenticate(Some(&token)).expect("身份");
    let (entered, started) = std::sync::mpsc::channel();
    let (resume, release) = std::sync::mpsc::channel();
    let tools = ConnectorTools::new(
        auth.clone(),
        CardSigner::new(&auth.signer_key().expect("密钥")),
        Arc::new(Source {
            entered,
            release: Mutex::new(release),
        }),
    );
    let call = std::thread::spawn(move || {
        tools.call(
            &identity,
            "create_card",
            json!({"operation_id":"op","workspace_id":"w","area_id":"a","profile":"codex"}),
        )
    });
    started
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("已进入执行边界");
    auth.revoke().expect("吊销");
    resume.send(()).expect("释放后台");
    let result = call
        .join()
        .expect("工具线程")
        .structured_content
        .expect("响应");
    assert_eq!(
        result["error"]["code"], "unauthorized",
        "吊销后的原调用不能释放成功回执"
    );
}
