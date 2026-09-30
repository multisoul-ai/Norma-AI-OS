//! Background mobile pairing owner. Never routes raw local IPC commands from the network.
mod approval;
pub use approval::LiveApprovalRequest;
mod command;
mod thread_followup;
mod thread_task;
mod thread_task_execution;
mod thread_task_terminal;
mod thread_terminal_control;
pub use thread_task_terminal::Request as ThreadTerminalRequest;
pub use thread_terminal_control::{ThreadTerminalCommand, ThreadTerminalOperation};
mod thread_task_acceptance;
mod thread_task_review;
pub(crate) use thread_task::instruction as thread_task_instruction;
pub(crate) mod controller;
mod conversation;
mod terminal;
pub use terminal::{LiveTerminalRequest, LiveTerminalTask};
mod review;
pub use review::{LiveReviewRequest, ProjectReviewRequest};
mod plan;
pub(crate) mod project_events;
pub(crate) mod push;
mod queue;
pub use push::PushRelayConfig;
mod server;
mod snapshot;
mod todo;
use anyhow::{ensure, Result};
pub use command::{
    CommandLease, LiveCommandRequest, MobileCommand, MobileCommandRequest, MobileProvider,
};
pub use conversation::LiveConversationRequest;
use norma::mobile::{PairingPending, PairingQr, PairingRequest, PairingSession, ServerIdentity};
pub use server::MobileServer;
pub use snapshot::LiveSnapshotRequest;
use std::time::Instant;
use storage::{AppStore, MobileDeviceGrant, MobilePermission, MobileProjectAccess};

/// All methods belong on a serialized background owner: authentication and pairing perform IO.
/// Foreground UI controls pairing/permissions; the gateway separately validates remote commands.
pub struct MobileAccess {
    store: AppStore,
    identity: ServerIdentity,
    pairing: Option<PairingSession>,
}

impl MobileAccess {
    #[cfg(target_os = "macos")]
    pub fn open(store: AppStore) -> Result<Self> {
        let settings = store.settings_store();
        let identity = match settings.get_string("mobile.server_identity")? {
            Some(record) => ServerIdentity::from_storage_json(&record)?,
            None => {
                let generated = ServerIdentity::generate()?;
                let record = settings
                    .get_or_insert_string("mobile.server_identity", &generated.storage_json()?)?;
                ServerIdentity::from_storage_json(&record)?
            }
        };
        Ok(Self {
            store,
            identity,
            pairing: None,
        })
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn with_identity_for_e2e(store: AppStore, identity: ServerIdentity) -> Self {
        Self {
            store,
            identity,
            pairing: None,
        }
    }

    pub fn machine_id(&self) -> String {
        self.identity.machine_id()
    }

    pub fn server_config(&self) -> Result<std::sync::Arc<rustls::ServerConfig>> {
        self.identity.server_config()
    }

    /// LOCAL DESKTOP ONLY. A new QR invalidates the old QR and its pending claimant.
    pub fn begin_pairing(
        &mut self,
        name: &str,
        endpoints: Vec<String>,
        now: Instant,
    ) -> Result<PairingQr> {
        self.cancel_pairing();
        let (session, qr) = PairingSession::new(&self.identity, name, endpoints, now)?;
        self.pairing = Some(session);
        Ok(qr)
    }

    pub fn claim(&mut self, request: PairingRequest, now: Instant) -> Result<PairingPending> {
        self.pairing
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("pairing unavailable"))?
            .claim(request, now)
    }

    pub fn exchange_code(
        &mut self,
        message: &str,
        now: Instant,
    ) -> Result<norma::mobile::PairingCodeReply> {
        self.pairing
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("pairing unavailable"))?
            .exchange_code(message, now)
    }

    pub fn pending(&mut self, now: Instant) -> Option<PairingPending> {
        self.pairing.as_mut()?.pending(now)
    }

    /// LOCAL DESKTOP ONLY. Scope comes from the Mac selection, never the phone request.
    pub fn approve(
        &mut self,
        request_id: &str,
        projects: &[MobileProjectAccess],
        now: Instant,
    ) -> Result<MobileDeviceGrant> {
        self.validate_projects(projects)?;
        let store = self.store.mobile_access_store();
        self.pairing
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("pairing unavailable"))?
            .approve(request_id, now, |id, name, token| {
                Ok(store.register(id, name, token, projects)?)
            })
    }

    /// LOCAL DESKTOP ONLY. The Mac chooses this mode while approving the pending phone.
    pub fn approve_all_projects(
        &mut self,
        request_id: &str,
        access: MobilePermission,
        now: Instant,
    ) -> Result<MobileDeviceGrant> {
        let store = self.store.mobile_access_store();
        self.pairing
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("pairing unavailable"))?
            .approve(request_id, now, |id, name, token| {
                Ok(store.register_all_projects(id, name, token, access)?)
            })
    }

    pub fn cancel_pairing(&mut self) {
        if let Some(mut pairing) = self.pairing.take() {
            pairing.cancel();
        }
    }

    pub fn authenticate(
        &self,
        machine_id: &str,
        device_id: &str,
        token: &str,
    ) -> Result<Option<MobileDeviceGrant>> {
        if machine_id != self.machine_id() {
            return Ok(None);
        }
        // Fresh SQLite lookup on every authentication. Returned grants are snapshots: a later
        // action must check current revision plus target window/run/turn again before dispatch.
        Ok(self
            .store
            .mobile_access_store()
            .authenticate(device_id, token)?)
    }

    /// LOCAL DESKTOP ONLY.
    pub fn replace_permissions(
        &mut self,
        device_id: &str,
        revision: u64,
        projects: &[MobileProjectAccess],
    ) -> Result<MobileDeviceGrant> {
        self.validate_projects(projects)?;
        Ok(self
            .store
            .mobile_access_store()
            .replace_permissions(device_id, revision, projects)?)
    }

    /// LOCAL DESKTOP ONLY. Changing mode increments the grant revision.
    pub fn replace_with_all_projects(
        &mut self,
        device_id: &str,
        revision: u64,
        access: MobilePermission,
    ) -> Result<MobileDeviceGrant> {
        Ok(self
            .store
            .mobile_access_store()
            .replace_with_all_projects(device_id, revision, access)?)
    }

    /// LOCAL DESKTOP ONLY. The network owner must also close this device's active connections.
    pub fn revoke(&mut self, device_id: &str) -> Result<MobileDeviceGrant> {
        Ok(self.store.mobile_access_store().revoke(device_id)?)
    }

    pub fn devices(&self) -> Result<Vec<MobileDeviceGrant>> {
        Ok(self.store.mobile_access_store().list()?)
    }

    fn validate_projects(&self, projects: &[MobileProjectAccess]) -> Result<()> {
        ensure!(projects.len() <= 256, "too many mobile projects");
        let identities = self.store.workspace_identity_store();
        for project in projects {
            ensure!(
                identities.get(&project.workspace_id)?.is_some(),
                "unknown mobile project"
            );
        }
        Ok(())
    }
}

/// Bounded in-process sources owned by the desktop controller; no network/Provider handles here.
#[derive(Default)]
pub struct MobileSources {
    pub terminals: Option<async_channel::Sender<LiveTerminalRequest>>,
    pub reviews: Option<async_channel::Sender<LiveReviewRequest>>,
    pub snapshots: Option<async_channel::Sender<LiveSnapshotRequest>>,
    pub commands: Option<async_channel::Sender<LiveCommandRequest>>,
    pub conversations: Option<async_channel::Sender<LiveConversationRequest>>,
    pub approvals: Option<async_channel::Sender<LiveApprovalRequest>>,
}
