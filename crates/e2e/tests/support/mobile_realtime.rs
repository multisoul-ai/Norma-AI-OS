use app::{LiveCommandRequest, LiveSnapshotRequest, MobileAccess, MobileServer};
use norma::mobile::{ProjectSnapshot, ServerIdentity};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use storage::{AppStore, MobilePermission, MobileProjectAccess};
#[path = "realtime_upstream.rs"]
mod upstream;
pub use upstream::*;
#[path = "mobile.rs"]
pub(super) mod wire;

pub struct Fixture {
    pub server: MobileServer,
    pub store: AppStore,
    pub workspace: String,
    pub machine: String,
    pub cert: Vec<u8>,
    pub upstream: std::net::TcpListener,
    pub writes: Arc<AtomicUsize>,
    stop: std::sync::mpsc::Sender<()>,
    frontend: Option<std::thread::JoinHandle<()>>,
    _directory: tempfile::TempDir,
}
impl Fixture {
    pub fn operations(&self, workspace: &str) -> Value {
        let mut phone = wire::connect(self.server.addresses()[0], self.cert.clone());
        wire::exchange(
            &mut phone,
            &self.machine,
            "operations",
            json!({"type":"realtime_operations","device_id":"phone-a","token":"a".repeat(64),
                "workspace_id":workspace,"after":0}),
        )
    }
    pub fn new(
        mut before_snapshot: impl FnMut() + Send + 'static,
        mut on_command: impl FnMut(LiveCommandRequest) + Send + 'static,
    ) -> Self {
        let directory = tempfile::tempdir().expect("隔离目录");
        let store = AppStore::new(directory.path().join("app.sqlite"));
        let workspace = store
            .workspace_identity_store()
            .resolve_or_create(directory.path(), "A")
            .expect("项目")
            .workspace_id;
        store
            .mobile_access_store()
            .register(
                "phone-a",
                "A",
                &"a".repeat(64),
                &[MobileProjectAccess {
                    workspace_id: workspace.clone(),
                    access: MobilePermission::Control,
                }],
            )
            .expect("控制授权");
        let (snapshots, snapshot_requests) = async_channel::bounded::<LiveSnapshotRequest>(8);
        let (commands, command_requests) = async_channel::bounded::<LiveCommandRequest>(8);
        let count = Arc::new(AtomicUsize::new(0));
        let writes = Arc::clone(&count);
        let (stop, stopped) = std::sync::mpsc::channel();
        let frontend = std::thread::spawn(move || {
            loop {
                if stopped.try_recv().is_ok() {
                    break;
                }
                while let Ok(request) = snapshot_requests.try_recv() {
                    before_snapshot();
                    let projects = request
                        .projects
                        .into_iter()
                        .map(|p| ProjectSnapshot {
                            state: "open".into(),
                            route_id: Some("route-a".into()),
                            cards: Some(vec![]),
                            card_count: Some(0),
                            ..p
                        })
                        .collect();
                    let _ = request.reply.send(projects);
                }
                if let Ok(request) = command_requests.try_recv() {
                    assert!(request.lease.begin(), "唯一前台执行权必须有效");
                    writes.fetch_add(1, Ordering::SeqCst);
                    on_command(request);
                }
                // Only a test foreground substitute polls: 2ms bounds fixture scheduling below wire deadlines.
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let identity = ServerIdentity::generate().expect("测试证书");
        let cert = identity.certificate_der().to_vec();
        let machine = identity.machine_id();
        let server = MobileServer::start_with_sources(
            MobileAccess::with_identity_for_e2e(store.clone(), identity),
            vec!["127.0.0.1:0".parse().expect("回环")],
            Some(snapshots),
            Some(commands),
        )
        .expect("手机服务");
        let upstream = std::net::TcpListener::bind("127.0.0.1:0").expect("上游回环");
        server
            .use_realtime_server_for_e2e(upstream.local_addr().expect("地址"))
            .expect("隔离上游");
        Self {
            server,
            store,
            workspace,
            machine,
            cert,
            upstream,
            writes: count,
            stop,
            frontend: Some(frontend),
            _directory: directory,
        }
    }
    pub fn bind(
        &self,
        generation: u64,
    ) -> tungstenite::WebSocket<rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream>>
    {
        let mut phone = wire::connect(self.server.addresses()[0], self.cert.clone());
        let _ = wire::exchange(
            &mut phone,
            &self.machine,
            "bind",
            json!({"type":"bind_realtime","device_id":"phone-a","token":"a".repeat(64),
            "binding":{"call_id":"rtc_A","workspace_id":self.workspace,"generation":generation}}),
        );
        phone
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(frontend) = self.frontend.take() {
            frontend.join().expect("前台替身收尾");
        }
        self.server.shutdown();
    }
}

pub fn accepted(request: LiveCommandRequest) {
    match request.request.command {
        app::MobileCommand::RealtimeTool {
            command: agent::AdapterCommand::CreateAgent { .. },
        } => {
            let receipt = agent::AdapterResult::accepted("已接收，执行尚未完成")
                .with_task_id(7)
                .with_node_id(3);
            request
                .reply
                .send(Ok(serde_json::to_value(receipt).expect("接收回执")))
                .expect("返回真实通道");
        }
        _ => panic!("实时入口只能投递经过解析的预期内部命令"),
    }
}
