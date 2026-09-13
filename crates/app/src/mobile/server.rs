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

mod connection;

/// App-owned endpoint. Construction and methods perform IO: call only on background workers.
/// The process owns this handle until disabled or App quit; there is no independent daemon.
pub struct MobileServer {
    shared: Arc<Shared>,
    addresses: Vec<SocketAddr>,
}

pub(super) struct Shared {
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
}

struct ActiveSocket {
    stream: TcpStream,
    device_id: Option<String>,
}

impl MobileServer {
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
            }),
            addresses,
        };
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
    pub(super) fn command_lease(
        &self,
        device: &str,
        operation: &str,
    ) -> Result<Arc<CommandLease>, &'static str> {
        let mut leases = self.command_leases.lock().map_err(|_| "unavailable")?;
        leases.retain(|_, values| {
            values.retain(|lease| lease.strong_count() > 0);
            !values.is_empty()
        });
        if self.stopped.load(Ordering::Acquire) {
            return Err("unavailable");
        }
        if leases.values().map(Vec::len).sum::<usize>() >= 8 {
            return Err("busy");
        }
        let lease = Arc::new(CommandLease::new(operation));
        leases
            .entry(device.into())
            .or_default()
            .push(Arc::downgrade(&lease));
        Ok(lease)
    }
    pub(super) fn command_pending(&self, device: &str, operation: &str) -> bool {
        self.command_leases
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(device)
            .is_some_and(|leases| {
                leases.iter().filter_map(Weak::upgrade).any(|lease| {
                    lease.operation_id == operation && !lease.finished.load(Ordering::Acquire)
                })
            })
    }

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
