use app::{MobileAccess, MobileServer};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// Two actual pinned Mac services; only the named isolated test phone is approved.
pub struct Targets {
    pub config: Vec<Value>,
    stopped: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
    servers: Vec<Arc<MobileServer>>,
    _directory: tempfile::TempDir,
}

impl Targets {
    pub fn new() -> Self {
        Self::with_primary_identity(None)
    }

    pub fn with_primary_identity(mut identity: Option<norma::mobile::ServerIdentity>) -> Self {
        let directory = tempfile::tempdir().expect("target fixture");
        let stopped = Arc::new(AtomicBool::new(false));
        let mut result = Self {
            config: vec![],
            stopped,
            workers: vec![],
            servers: vec![],
            _directory: directory,
        };
        for name in ["A", "B"] {
            let store = AppStore::new(result._directory.path().join(format!("{name}.sqlite")));
            let mut grants = Vec::new();
            let mut ids = Vec::new();
            for (suffix, access) in [
                ("write-a", MobilePermission::Control),
                ("write-b", MobilePermission::Control),
                ("read-c", MobilePermission::Read),
            ] {
                let path = result._directory.path().join(format!("{name}-{suffix}"));
                std::fs::create_dir(&path).expect("target project");
                let id = store
                    .workspace_identity_store()
                    .resolve_or_create(&path, &format!("{name}-{suffix}"))
                    .expect("target identity")
                    .workspace_id;
                grants.push(MobileProjectAccess {
                    workspace_id: id.clone(),
                    access,
                });
                ids.push(id);
            }
            let (source, snapshots) = async_channel::bounded::<app::LiveSnapshotRequest>(8);
            let server = Arc::new(
                MobileServer::start_with_sources(
                    MobileAccess::with_identity_for_e2e(
                        store.clone(),
                        identity.take().unwrap_or_else(|| {
                            norma::mobile::ServerIdentity::generate().expect("isolated identity")
                        }),
                    ),
                    vec!["127.0.0.1:0".parse().expect("loopback")],
                    Some(source),
                    None,
                )
                .expect("target server"),
            );
            let offer: Value = serde_json::from_str(
                &server
                    .begin_pairing(&format!("语音验收 Mac {name}"))
                    .expect("pairing")
                    .to_json()
                    .expect("offer json"),
            )
            .expect("offer");
            result
                .config
                .push(json!({"offer":offer,"writeA":ids[0],"writeB":ids[1],"readC":ids[2]}));
            let (peer, stop) = (server.clone(), result.stopped.clone());
            result.workers.push(thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    while let Ok(request) = snapshots.try_recv() {
                        let projects = request
                            .projects
                            .into_iter()
                            .map(|mut project| {
                                project.state = "closed".into();
                                project
                            })
                            .collect();
                        let _ = request.reply.send(projects);
                    }
                    if let Some(pending) = peer.pending().expect("test pairing request") {
                        assert_eq!(
                            pending.device_name, "Norma Voice Target Test",
                            "只允许专用测试手机配对"
                        );
                        peer.approve(&pending.request_id, &grants)
                            .expect("approve isolated test projects");
                        // One historical, unfinished reservation per test Mac. No command is dispatched.
                        // The phone must preserve/reconcile this identity without submitting anything.
                        let ledger = storage::MobileCommandStore::new(store.path().to_path_buf());
                        let record = storage::mobile_command_store::RealtimeCall {
                            device_id: pending.device_id.clone(),
                            call_id: format!("rtc_recovery_{name}"),
                            turn_id: "historical-utterance".into(),
                            workspace_id: ids[0].clone(),
                            operation_id: format!("rt-pending-{name}"),
                            request_hash: "a".repeat(64),
                        };
                        assert!(
                            matches!(
                                ledger
                                    .reserve_realtime(&record)
                                    .expect("historical reservation"),
                                storage::mobile_command_store::RealtimeReservation::New
                            ),
                            "fixture must seed exactly one unexecuted historical operation"
                        );
                    }
                    // 20ms = one capture frame; only fixture pairing approval polls.
                    thread::sleep(Duration::from_millis(20));
                }
            }));
            result.servers.push(server);
        }
        result
    }
}

impl Drop for Targets {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        for worker in self.workers.drain(..) {
            worker.join().expect("approval worker shutdown");
        }
        for server in &self.servers {
            server.shutdown();
        }
    }
}
