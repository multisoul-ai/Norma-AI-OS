use super::{MobileAccess, MobileServer};
use gpui::{App, AppContext, Context, Entity, Global, RenderImage, Task, WeakEntity};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use storage::{AppStore, MobileDeviceGrant, MobileProjectAccess, WorkspaceIdentity};

mod work;
use work::Completion;

#[derive(Default)]
struct Controllers(HashMap<PathBuf, WeakEntity<MobileController>>);
impl Global for Controllers {}

/// Only window entities hold strong references; the process registry never keeps a server alive.
pub(crate) fn attach(store: AppStore, cx: &mut App) -> Entity<MobileController> {
    if !cx.has_global::<Controllers>() {
        cx.set_global(Controllers::default());
    }
    let registry = cx.global_mut::<Controllers>();
    registry.0.retain(|_, entity| entity.upgrade().is_some());
    if let Some(existing) = registry.0.get(store.path()).and_then(WeakEntity::upgrade) {
        return existing;
    }
    let path = store.path().to_path_buf();
    let entity = cx.new(|cx| {
        let mut host = MobileController::new(store);
        let (sender, receiver) = async_channel::bounded::<super::LiveSnapshotRequest>(8);
        host.snapshots = Some(sender);
        let database = host.store.path().to_path_buf();
        host.snapshot_task = Some(cx.spawn(async move |_, cx| {
            while let Ok(request) = receiver.recv().await {
                if Instant::now() >= request.deadline {
                    continue;
                }
                let result = cx.update(|cx| {
                    crate::norma::endpoint::mobile_project_snapshots(
                        &database,
                        request.projects,
                        request.include_cards,
                        cx,
                    )
                });
                if let Ok(projects) = result {
                    let _ = request.reply.try_send(projects);
                }
            }
        }));
        let (sender, receiver) = async_channel::bounded::<super::LiveCommandRequest>(8);
        host.commands = Some(sender);
        let database = host.store.path().to_path_buf();
        host.command_task = Some(cx.spawn(async move |_, cx| {
            while let Ok(call) = receiver.recv().await {
                let _ = cx.update(|cx| {
                    crate::norma::endpoint::mobile_command::execute(database.clone(), call, cx)
                });
            }
        }));
        let (sender, receiver) = async_channel::bounded::<super::LiveTerminalRequest>(8);
        host.terminals = Some(sender);
        let database = host.store.path().to_path_buf();
        host.terminal_task = Some(cx.spawn(async move |_, cx| {
            while let Ok(call) = receiver.recv().await {
                let _ = cx.update(|cx| {
                    crate::norma::endpoint::mobile_terminal::read(&database, call, cx)
                });
            }
        }));
        let (sender, receiver) = async_channel::bounded::<super::LiveConversationRequest>(8);
        host.conversations = Some(sender);
        let database = host.store.path().to_path_buf();
        host.conversation_task = Some(cx.spawn(async move |_, cx| {
            while let Ok(call) = receiver.recv().await {
                let _ = cx.update(|cx| {
                    crate::norma::endpoint::mobile_conversation::read(database.clone(), call, cx)
                });
            }
        }));
        let (sender, receiver) = async_channel::bounded::<super::LiveApprovalRequest>(8);
        host.approvals = Some(sender);
        let database = host.store.path().to_path_buf();
        host.approval_task = Some(cx.spawn(async move |_, cx| {
            while let Ok(call) = receiver.recv().await {
                let _ = cx.update(|cx| {
                    crate::norma::endpoint::mobile_approval::read(&database, call, cx)
                });
            }
        }));
        let (sender, receiver) = async_channel::bounded::<super::LiveReviewRequest>(8);
        host.reviews = Some(sender);
        let database = host.store.path().to_path_buf();
        host.review_task = Some(cx.spawn(async move |_, cx| {
            while let Ok(call) = receiver.recv().await {
                let _ = cx.update(|cx| {
                    crate::norma::endpoint::mobile_review::read(database.clone(), call, cx)
                });
            }
        }));
        host
    });
    cx.global_mut::<Controllers>()
        .0
        .insert(path, entity.downgrade());
    entity
}

#[derive(Clone)]
pub(crate) enum Command {
    Reload,
    Enable,
    NewCode,
    CancelCode,
    LoadPush,
    SavePush {
        endpoint: String,
        key: String,
        environment: storage::PushEnvironment,
    },
    ClearPush,
    Approve {
        request_id: String,
        projects: Vec<MobileProjectAccess>,
    },
    Revoke(String),
    Permissions {
        device_id: String,
        revision: u64,
        projects: Vec<MobileProjectAccess>,
    },
}

pub(crate) struct MobileController {
    store: AppStore,
    server: Option<Arc<MobileServer>>,
    cancelled: Arc<AtomicBool>,
    generation: u64,
    operation: Option<Task<()>>,
    polling: Option<Task<()>>,
    snapshots: Option<async_channel::Sender<super::LiveSnapshotRequest>>,
    snapshot_task: Option<Task<()>>,
    commands: Option<async_channel::Sender<super::LiveCommandRequest>>,
    command_task: Option<Task<()>>,
    terminals: Option<async_channel::Sender<super::LiveTerminalRequest>>,
    terminal_task: Option<Task<()>>,
    conversations: Option<async_channel::Sender<super::LiveConversationRequest>>,
    conversation_task: Option<Task<()>>,
    reviews: Option<async_channel::Sender<super::LiveReviewRequest>>,
    review_task: Option<Task<()>>,
    approvals: Option<async_channel::Sender<super::LiveApprovalRequest>>,
    approval_task: Option<Task<()>>,
    pub(crate) busy: bool,
    pub(crate) error: Option<String>,
    pub(crate) push: Option<super::push::configuration::Summary>,
    pub(crate) projects: Vec<WorkspaceIdentity>,
    pub(crate) devices: Vec<MobileDeviceGrant>,
    pub(crate) pending: Option<norma::mobile::PairingPending>,
    pub(crate) qr: Option<Arc<RenderImage>>,
    pub(crate) pairing_code: Option<String>,
    pub(crate) code_copied: bool,
    expires: Option<Instant>,
    pub(crate) seconds_left: u64,
    pub(crate) selected: HashMap<String, storage::MobilePermission>,
    pub(crate) editing: Option<(String, u64)>,
}

impl MobileController {
    fn new(store: AppStore) -> Self {
        Self {
            store,
            server: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            generation: 0,
            operation: None,
            polling: None,
            snapshots: None,
            snapshot_task: None,
            commands: None,
            command_task: None,
            terminals: None,
            terminal_task: None,
            conversations: None,
            conversation_task: None,
            reviews: None,
            review_task: None,
            approvals: None,
            approval_task: None,
            busy: false,
            error: None,
            push: None,
            projects: vec![],
            devices: vec![],
            pending: None,
            qr: None,
            pairing_code: None,
            code_copied: false,
            expires: None,
            seconds_left: 0,
            selected: HashMap::new(),
            editing: None,
        }
    }
    pub(crate) fn enabled(&self) -> bool {
        self.server.is_some()
    }
    pub(crate) fn review_source(&self) -> Option<async_channel::Sender<super::LiveReviewRequest>> {
        self.reviews.clone()
    }
    pub(crate) fn snapshot_source(
        &self,
    ) -> Option<async_channel::Sender<super::LiveSnapshotRequest>> {
        self.snapshots.clone()
    }
    pub(crate) fn approval_source(
        &self,
    ) -> Option<async_channel::Sender<super::LiveApprovalRequest>> {
        self.approvals.clone()
    }
    pub(crate) fn terminal_source(
        &self,
    ) -> Option<async_channel::Sender<super::LiveTerminalRequest>> {
        self.terminals.clone()
    }
    pub(crate) fn conversation_source(
        &self,
    ) -> Option<async_channel::Sender<super::LiveConversationRequest>> {
        self.conversations.clone()
    }
    pub(crate) fn command_source(
        &self,
    ) -> Option<async_channel::Sender<super::LiveCommandRequest>> {
        self.commands.clone()
    }
    pub(crate) fn addresses(&self) -> Vec<String> {
        self.server
            .as_ref()
            .map(|s| s.addresses().iter().map(ToString::to_string).collect())
            .unwrap_or_default()
    }
    pub(crate) fn disable(&mut self, cx: &mut Context<Self>) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(server) = self.server.take() {
            server.shutdown();
        }
        self.generation += 1;
        self.operation = None;
        self.polling = None;
        self.busy = false;
        self.clear_pairing();
        self.error = None;
        cx.notify();
    }
    fn clear_pairing(&mut self) {
        self.qr = None;
        self.pairing_code = None;
        self.code_copied = false;
        self.expires = None;
        self.pending = None;
        self.selected.clear();
        self.editing = None;
        self.seconds_left = 0;
    }
    pub(crate) fn copy_pairing_code(&mut self, cx: &mut Context<Self>) {
        if self.busy
            || self.pending.is_some()
            || self.expires.is_none_or(|end| end <= Instant::now())
        {
            return;
        }
        if let Some(code) = &self.pairing_code {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(code.clone()));
            self.code_copied = true;
            cx.notify();
        }
    }
    pub(crate) fn chosen_projects(&self) -> Vec<MobileProjectAccess> {
        self.projects
            .iter()
            .filter_map(|project| {
                self.selected
                    .get(&project.workspace_id)
                    .map(|access| MobileProjectAccess {
                        workspace_id: project.workspace_id.clone(),
                        access: *access,
                    })
            })
            .collect()
    }
    pub(crate) fn edit(&mut self, device: &MobileDeviceGrant, cx: &mut Context<Self>) {
        if self.busy || self.qr.is_some() {
            return;
        }
        self.selected = device
            .projects
            .iter()
            .map(|p| (p.workspace_id.clone(), p.access))
            .collect();
        self.editing = Some((device.device_id.clone(), device.revision));
        cx.notify();
    }
    pub(crate) fn request(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if matches!(command, Command::Reload) && self.editing.take().is_some() {
            self.selected.clear();
        }
        if matches!(command, Command::Enable) {
            if self.enabled() {
                return;
            }
            self.cancelled = Arc::new(AtomicBool::new(false));
        }
        if matches!(command, Command::NewCode | Command::Enable) {
            self.clear_pairing();
        }
        if let Command::Approve { request_id, .. } = &command {
            if self.pending.as_ref().map(|p| &p.request_id) != Some(request_id) {
                self.error = Some("配对申请已更新，请重新核对".into());
                cx.notify();
                return;
            }
        }
        self.busy = true;
        self.error = None;
        self.generation += 1;
        let generation = self.generation;
        let store = self.store.clone();
        let server = self.server.clone();
        let cancelled = self.cancelled.clone();
        let sources = super::MobileSources {
            terminals: self.terminal_source(),
            snapshots: self.snapshot_source(),
            commands: self.command_source(),
            conversations: self.conversation_source(),
            approvals: self.approval_source(),
            reviews: self.review_source(),
        };
        let job = cx
            .background_executor()
            .spawn(async move { work::execute(command, store, server, cancelled, sources) });
        self.operation = Some(cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |host, cx| {
                if generation != host.generation {
                    return;
                }
                host.busy = false;
                match result {
                    Ok(Completion::Push(summary)) => host.push = Some(summary),
                    Ok(Completion::Info(projects, devices)) => {
                        host.projects = projects;
                        host.devices = devices;
                    }
                    Ok(Completion::Started(server, image, code, expires, projects, devices)) => {
                        host.server = Some(server);
                        host.projects = projects;
                        host.devices = devices;
                        host.install_code(image, code, expires, cx);
                    }
                    Ok(Completion::Code(image, code, expires)) => {
                        host.install_code(image, code, expires, cx)
                    }
                    Ok(Completion::Cancelled) => host.clear_pairing(),
                    Ok(Completion::Approved(devices)) => {
                        host.devices = devices;
                        host.clear_pairing();
                    }
                    Ok(Completion::Devices(devices)) => {
                        host.devices = devices;
                        host.editing = None;
                    }
                    Err(error) => host.error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn install_code(
        &mut self,
        image: Arc<RenderImage>,
        code: String,
        expires: Instant,
        cx: &mut Context<Self>,
    ) {
        self.clear_pairing();
        self.qr = Some(image);
        self.pairing_code = Some(code);
        self.expires = Some(expires);
        self.seconds_left = expires.saturating_duration_since(Instant::now()).as_secs();
        self.polling = Some(cx.spawn(async move |this, cx| {
            loop {
                // Only an active two-minute QR needs polling; no idle service/UI timer.
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let Ok(Some((server, generation))) = this.update(cx, |host, _| {
                    host.qr.as_ref()?;
                    Some((host.server.clone()?, host.generation))
                }) else {
                    break;
                };
                let pending = cx
                    .background_executor()
                    .spawn(async move { server.pending() })
                    .await;
                let keep = this.update(cx, |host, cx| {
                    if host.qr.is_none() {
                        return false;
                    }
                    if generation != host.generation {
                        return true;
                    }
                    host.seconds_left = host
                        .expires
                        .unwrap_or_else(Instant::now)
                        .saturating_duration_since(Instant::now())
                        .as_secs();
                    if host.seconds_left == 0 {
                        host.clear_pairing();
                    } else {
                        match pending {
                            Ok(pending) => host.pending = pending,
                            Err(error) => {
                                host.error = Some(error.to_string());
                                host.clear_pairing();
                            }
                        }
                    }
                    cx.notify();
                    host.qr.is_some()
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        }));
    }
}

impl Drop for MobileController {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(server) = &self.server {
            server.shutdown();
        }
    }
}

#[cfg(all(test, feature = "e2e-test-support"))]
#[path = "../../tests/unit/mobile_desktop_tests.rs"]
mod tests;
