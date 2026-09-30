use super::{CommandLease, LiveCommandRequest, LiveSnapshotRequest, MobileAccess};
use anyhow::{ensure, Result};
use norma::mobile::{PairingPending, PairingQr};
use std::{
    collections::HashMap,
    net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, Weak,
    },
    time::{Duration, Instant},
};
use storage::{MobileDeviceGrant, MobileProjectAccess};

mod command_leases;
mod connection;
mod relay;
mod terminal_leases;

/// App-owned endpoint. Construction and methods perform IO: call only on background workers.
/// The process owns this handle until disabled or App quit; there is no independent daemon.
pub struct MobileServer {
    shared: Arc<Shared>,
    addresses: Vec<SocketAddr>,
}

pub(super) struct Shared {
    relay: Mutex<Option<relay::Control>>,
    #[cfg(feature = "e2e-test-support")]
    realtime_test_server: Mutex<Option<SocketAddr>>,
    access: Mutex<MobileAccess>,
    pub(super) stopped: AtomicBool,
    pub(super) push_started: AtomicBool,
    pub(super) push_delivery: Mutex<super::push::Delivery>,
    sockets: Mutex<HashMap<u64, ActiveSocket>>,
    next_socket: AtomicU64,
    pub(super) snapshots: Option<async_channel::Sender<LiveSnapshotRequest>>,
    pub(super) commands: Option<async_channel::Sender<LiveCommandRequest>>,
    pub(super) terminals: Option<async_channel::Sender<super::LiveTerminalRequest>>,
    pub(super) conversations: Option<async_channel::Sender<super::LiveConversationRequest>>,
    pub(super) reviews: Option<async_channel::Sender<super::LiveReviewRequest>>,
    pub(super) approvals: Option<async_channel::Sender<super::LiveApprovalRequest>>,
    pub(super) owner_id: String,
    command_leases: Mutex<HashMap<String, Vec<Weak<CommandLease>>>>,
    terminal_leases: Mutex<HashMap<String, Vec<Weak<runtime::TerminalInputLease>>>>,
}

struct ActiveSocket {
    stream: TcpStream,
    device_id: Option<String>,
    realtime: Option<Arc<connection::realtime::Session>>,
}

impl MobileServer {
    #[cfg(feature = "e2e-test-support")]
    pub fn use_realtime_server_for_e2e(&self, address: SocketAddr) -> Result<()> {
        ensure!(
            address.ip().is_loopback(),
            "realtime fixture must use loopback"
        );
        *self
            .shared
            .realtime_test_server
            .lock()
            .map_err(|_| anyhow::anyhow!("test server lock"))? = Some(address);
        Ok(())
    }
    #[cfg(feature = "e2e-test-support")]
    pub fn submit_internal_command_for_e2e(
        &self,
        machine: &str,
        device: &str,
        token: &str,
        request: super::MobileCommandRequest,
    ) -> Result<serde_json::Value, &'static str> {
        super::command::submit(&self.shared, machine, device, token, request)
    }

    pub fn start(access: MobileAccess, addresses: Vec<SocketAddr>) -> Result<Self> {
        Self::start_with_snapshots(access, addresses, None)
    }

    pub fn start_with_snapshots(
        access: MobileAccess,
        addresses: Vec<SocketAddr>,
        snapshots: Option<async_channel::Sender<LiveSnapshotRequest>>,
    ) -> Result<Self> {
        Self::start_with_sources(access, addresses, snapshots, None)
    }

    pub fn start_with_sources(
        access: MobileAccess,
        addresses: Vec<SocketAddr>,
        snapshots: Option<async_channel::Sender<LiveSnapshotRequest>>,
        commands: Option<async_channel::Sender<LiveCommandRequest>>,
    ) -> Result<Self> {
        Self::start_with_conversations(access, addresses, snapshots, commands, None)
    }

    pub fn start_with_conversations(
        access: MobileAccess,
        addresses: Vec<SocketAddr>,
        snapshots: Option<async_channel::Sender<LiveSnapshotRequest>>,
        commands: Option<async_channel::Sender<LiveCommandRequest>>,
        conversations: Option<async_channel::Sender<super::LiveConversationRequest>>,
    ) -> Result<Self> {
        Self::start_with_mobile_sources(
            access,
            addresses,
            super::MobileSources {
                terminals: None,
                snapshots,
                commands,
                conversations,
                approvals: None,
                reviews: None,
            },
        )
    }

    pub fn start_with_mobile_sources(
        access: MobileAccess,
        addresses: Vec<SocketAddr>,
        sources: super::MobileSources,
    ) -> Result<Self> {
        let super::MobileSources {
            terminals,
            snapshots,
            commands,
            conversations,
            approvals,
            reviews,
        } = sources;
        ensure!(
            !addresses.is_empty() && addresses.len() <= 4,
            "invalid mobile listen addresses"
        );
        #[cfg(feature = "e2e-test-support")]
        let realtime_fixture =
            if let Some(upstream) = std::env::var_os("NORMA_REALTIME_L3_UPSTREAM") {
                ensure!(
                    !cfg!(feature = "production-data")
                        && std::env::var("NORMA_MOBILE_L3_LOOPBACK").as_deref() == Ok("1")
                        && addresses.iter().all(|address| address.ip().is_loopback()),
                    "realtime bundle fixture requires isolated loopback mode"
                );
                let root =
                    std::path::PathBuf::from(std::env::var_os("NORMA_TEST_DATA_ROOT").ok_or_else(
                        || anyhow::anyhow!("realtime fixture requires an isolated database"),
                    )?);
                ensure!(
                    root.is_absolute()
                        && root.join("app.sqlite").canonicalize()?
                            == access.store.path().canonicalize()?
                        && crate::database::default_database_paths()?
                            .app_db_path()
                            .canonicalize()?
                            == access.store.path().canonicalize()?,
                    "realtime fixture database mismatch"
                );
                Some((
                    upstream
                        .to_str()
                        .ok_or_else(|| anyhow::anyhow!("invalid fixture address"))?
                        .parse::<SocketAddr>()?,
                    root,
                ))
            } else {
                None
            };
        let tls = access.server_config()?;
        let mut listeners = Vec::new();
        for address in addresses {
            ensure!(
                allowed_address(address.ip()),
                "mobile requires an explicit LAN, Tailscale or loopback address"
            );
            let listener = TcpListener::bind(address)?;
            listener.set_nonblocking(true)?;
            listeners.push(listener);
        }
        let addresses = listeners
            .iter()
            .map(TcpListener::local_addr)
            .collect::<std::io::Result<Vec<_>>>()?;
        let server = Self {
            shared: Arc::new(Shared {
                relay: Mutex::new(None),
                #[cfg(feature = "e2e-test-support")]
                realtime_test_server: Mutex::new(None),
                access: Mutex::new(access),
                stopped: AtomicBool::new(false),
                push_started: AtomicBool::new(false),
                push_delivery: Mutex::new(super::push::Delivery::default()),
                sockets: Mutex::new(HashMap::new()),
                next_socket: AtomicU64::new(1),
                snapshots,
                commands,
                conversations,
                terminals,
                approvals,
                reviews,
                owner_id: crate::norma::endpoint::new_workspace_route_id()?,
                command_leases: Mutex::new(HashMap::new()),
                terminal_leases: Mutex::new(HashMap::new()),
            }),
            addresses,
        };
        #[cfg(feature = "e2e-test-support")]
        if let Some((address, root)) = realtime_fixture {
            server.use_realtime_server_for_e2e(address)?;
            let access = server.shared.access()?;
            std::fs::write(
                root.join("realtime-public-identity.json"),
                serde_json::to_vec(&serde_json::json!({
                    "machine_id": access.identity.machine_id(),
                    "certificate_der": access.identity.certificate_der(),
                }))?,
            )?;
        }
        #[cfg(feature = "e2e-test-support")]
        if std::env::var("NORMA_THREAD_CODEX_REAL_L3").as_deref() == Ok("1") {
            let root = std::path::PathBuf::from(
                std::env::var_os("NORMA_TEST_DATA_ROOT")
                    .ok_or_else(|| anyhow::anyhow!("real thread L3 requires isolated data"))?,
            );
            ensure!(
                !cfg!(feature = "production-data")
                    && root.is_absolute()
                    && std::env::var("NORMA_MOBILE_L3_LOOPBACK").as_deref() == Ok("1")
                    && server
                        .addresses
                        .iter()
                        .all(|address| address.ip().is_loopback())
                    && root.join("app.sqlite").canonicalize()?
                        == server.shared.access()?.store.path().canonicalize()?,
                "real thread L3 requires isolated loopback App"
            );
            let access = server.shared.access()?;
            std::fs::write(
                root.join("thread-real-public-identity.json"),
                serde_json::to_vec(
                    &serde_json::json!({"machine_id":access.identity.machine_id(),
                    "certificate_der":access.identity.certificate_der(), "addresses":server.addresses}),
                )?,
            )?;
        }
        for listener in listeners {
            let shared = Arc::clone(&server.shared);
            let tls = Arc::clone(&tls);
            std::thread::Builder::new()
                .name("norma-mobile-listen".into())
                .spawn(move || {
                    while !shared.stopped.load(Ordering::Acquire) {
                        match listener.accept() {
                            Ok((socket, _)) => {
                                connection::spawn(socket, Arc::clone(&tls), Arc::clone(&shared))
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                // ponytail: at most four private-interface listeners poll every 20ms;
                                // use one async accept loop if idle wakeups become measurable.
                                std::thread::sleep(Duration::from_millis(20));
                            }
                            Err(_) => {
                                shared.stop();
                                break;
                            }
                        }
                    }
                })?;
        }
        super::queue::start(Arc::clone(&server.shared))?;
        Ok(server)
    }

    pub fn addresses(&self) -> Vec<SocketAddr> {
        self.addresses.clone()
    }

    pub fn start_push_notifications(&self, config: super::PushRelayConfig) -> Result<()> {
        super::push::start(Arc::clone(&self.shared), config)
    }

    pub fn configure_push_notifications(
        &self,
        config: Option<super::PushRelayConfig>,
    ) -> Result<()> {
        super::push::configure(Arc::clone(&self.shared), config)
    }

    pub fn shutdown(&self) {
        self.shared.stop();
    }

    pub fn begin_pairing(&self, name: &str) -> Result<PairingQr> {
        let endpoints = self
            .addresses
            .iter()
            .map(|a| format!("wss://{a}/mobile"))
            .collect();
        self.shared
            .access()?
            .begin_pairing(name, endpoints, Instant::now())
    }

    pub fn pending(&self) -> Result<Option<PairingPending>> {
        Ok(self.shared.access()?.pending(Instant::now()))
    }

    pub fn approve(
        &self,
        request_id: &str,
        projects: &[MobileProjectAccess],
    ) -> Result<MobileDeviceGrant> {
        self.shared
            .access()?
            .approve(request_id, projects, Instant::now())
    }

    pub fn approve_all_projects(
        &self,
        request_id: &str,
        access: storage::MobilePermission,
    ) -> Result<MobileDeviceGrant> {
        self.shared
            .access()?
            .approve_all_projects(request_id, access, Instant::now())
    }

    pub fn cancel_pairing(&self) -> Result<()> {
        self.shared.access()?.cancel_pairing();
        Ok(())
    }

    pub fn replace_permissions(
        &self,
        device: &str,
        revision: u64,
        projects: &[MobileProjectAccess],
    ) -> Result<MobileDeviceGrant> {
        let mut access = self.shared.access()?;
        let grant = access.replace_permissions(device, revision, projects)?;
        self.shared.disconnect(device);
        Ok(grant)
    }

    pub fn replace_with_all_projects(
        &self,
        device: &str,
        revision: u64,
        access: storage::MobilePermission,
    ) -> Result<MobileDeviceGrant> {
        let mut owner = self.shared.access()?;
        let grant = owner.replace_with_all_projects(device, revision, access)?;
        self.shared.disconnect(device);
        Ok(grant)
    }

    pub fn revoke(&self, device: &str) -> Result<MobileDeviceGrant> {
        let mut access = self.shared.access()?;
        let grant = access.revoke(device)?;
        self.shared.disconnect(device);
        Ok(grant)
    }

    pub fn devices(&self) -> Result<Vec<MobileDeviceGrant>> {
        self.shared.access()?.devices()
    }
}

impl Drop for MobileServer {
    fn drop(&mut self) {
        self.shared.stop();
    }
}

impl Shared {
    pub(super) fn access(&self) -> Result<MutexGuard<'_, MobileAccess>> {
        let guard = self
            .access
            .lock()
            .map_err(|_| anyhow::anyhow!("mobile access unavailable"))?;
        ensure!(
            !self.stopped.load(Ordering::Acquire),
            "mobile server stopped"
        );
        Ok(guard)
    }

    fn disconnect(&self, device: &str) {
        self.revoke_terminal_leases(Some(device));
        // ponytail: permission edits reconnect this personal Mac's whole relay; split per-device
        // cancellation only if several simultaneous phones make those rare resets disruptive.
        if let Some(relay) = self
            .relay
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            relay.changed.notify_one();
        }
        if let Some(leases) = self
            .command_leases
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(device)
        {
            for lease in leases.iter().filter_map(Weak::upgrade) {
                lease.cancel();
            }
        }
        let mut sockets = self.sockets.lock().unwrap_or_else(|e| e.into_inner());
        sockets.retain(|_, active| {
            if active.device_id.as_deref() != Some(device) {
                return true;
            }
            let _ = active.stream.shutdown(Shutdown::Both);
            false
        });
        drop(sockets);
        super::project_events::changed();
    }

    pub(super) fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        self.revoke_terminal_leases(None);
        if let Some(relay) = self
            .relay
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            relay.cancelled.cancel();
        }
        super::project_events::changed();
        for (_, leases) in self
            .command_leases
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain()
        {
            for lease in leases.into_iter().filter_map(|lease| lease.upgrade()) {
                lease.cancel();
            }
        }
        for (_, socket) in self
            .sockets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain()
        {
            let _ = socket.stream.shutdown(Shutdown::Both);
        }
        // No waiting for a storage write or worker join on a foreground Drop. Requests check
        // stopped again after taking access; active sockets are already shut down above.
        if let Ok(mut access) = self.access.try_lock() {
            access.cancel_pairing();
        }
    }
}

fn allowed_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || (a == 100 && (64..=127).contains(&b))
        }
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(|ip| allowed_address(IpAddr::V4(ip)))
            .unwrap_or_else(|| {
                ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
            }),
    }
}
