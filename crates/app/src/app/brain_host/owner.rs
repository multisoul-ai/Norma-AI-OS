//! App-owned service and project subscriptions. No filesystem or process joins on this path.
use super::{config, BrainHost, START_RETRY};
use brain::acp::gate::ReadRoots;
use brain::{BrainEvent, BrainStatus};
use norma::protocol::BrainScope;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::Instant;

#[derive(Clone, Default)]
pub(super) struct SharedBrain(Rc<RefCell<Owner>>);
impl gpui::Global for SharedBrain {}

struct Member {
    id: u64,
    root: PathBuf,
    status: Sender<StatusEnvelope>,
}

struct StatusEnvelope {
    generation: u64,
    status: BrainStatus,
}

pub(super) struct ClientStatus {
    pub status: BrainStatus,
    pub retired: bool,
}

struct Retired {
    generation: u64,
    status: Receiver<BrainStatus>,
    stopped: Receiver<()>,
    joined: bool,
}

#[derive(Default)]
struct Owner {
    host: Option<BrainHost>,
    members: BTreeMap<BrainScope, Member>,
    next_member: u64,
    generation: u64,
    starting: Option<u64>,
    retry_at: Option<Instant>,
    settings: Option<config::BrainSettings>,
    directory: Option<PathBuf>,
    preparing: bool,
    prepare_retry_at: Option<Instant>,
    prepare_error: Option<String>,
    read_roots: ReadRoots,
    retired: Vec<Retired>,
}

pub(super) struct BrainClient {
    owner: SharedBrain,
    scope: BrainScope,
    id: u64,
    status: Receiver<StatusEnvelope>,
}

impl SharedBrain {
    pub(super) fn directory(&self) -> Option<PathBuf> {
        self.0.borrow().directory.clone()
    }

    pub(super) fn preparation_error(&self) -> Option<String> {
        self.0.borrow().prepare_error.clone()
    }

    pub(super) fn sources(&self) -> Vec<brain::migration::WorkspaceSource> {
        self.0
            .borrow()
            .members
            .iter()
            .map(|(scope, member)| brain::migration::WorkspaceSource {
                workspace_id: scope.workspace_id.clone(),
                root: member.root.clone(),
            })
            .collect()
    }

    pub(super) fn begin_prepare(&self, now: Instant) -> bool {
        let mut owner = self.0.borrow_mut();
        if owner.directory.is_some()
            || owner.preparing
            || owner
                .prepare_retry_at
                .is_some_and(|deadline| now < deadline)
        {
            return false;
        }
        owner.preparing = true;
        true
    }

    pub(super) fn finish_prepare(&self, result: Result<super::data::Prepared, String>) {
        let mut owner = self.0.borrow_mut();
        if !owner.preparing {
            return;
        }
        owner.preparing = false;
        match result {
            Ok(prepared) => {
                owner.directory = Some(prepared.directory);
                owner.settings = Some(prepared.settings);
                owner.prepare_error = None;
                owner.prepare_retry_at = None;
            }
            Err(reason) => {
                tracing::warn!(%reason, "Global brain data preparation failed; preserving original state");
                owner.prepare_error = Some(reason);
                owner.prepare_retry_at = Some(Instant::now() + START_RETRY);
            }
        }
    }

    pub(super) fn settings(&self) -> Option<config::BrainSettings> {
        self.0.borrow().settings.clone()
    }

    pub(super) fn configure(&self, settings: config::BrainSettings, restart: bool) {
        let mut owner = self.0.borrow_mut();
        let changed = owner.settings.as_ref() != Some(&settings);
        if changed && (restart || !settings.enabled) {
            owner.invalidate();
        }
        owner.settings = Some(settings);
    }

    pub(super) fn read_roots(&self) -> ReadRoots {
        self.0.borrow().read_roots.clone()
    }

    pub(super) fn register(&self, scope: BrainScope, root: PathBuf) -> BrainClient {
        let (sender, status) = mpsc::channel();
        let mut owner = self.0.borrow_mut();
        owner.next_member += 1;
        let id = owner.next_member;
        owner.members.insert(
            scope.clone(),
            Member {
                id,
                root,
                status: sender,
            },
        );
        owner.update_roots();
        BrainClient {
            owner: self.clone(),
            scope,
            id,
            status,
        }
    }

    pub(super) fn is_running(&self) -> bool {
        self.0.borrow().host.is_some()
    }

    pub(super) fn begin_start(&self, now: Instant) -> Option<u64> {
        let mut owner = self.0.borrow_mut();
        owner.poll();
        if owner.host.is_some()
            || owner.directory.is_none()
            || owner.starting.is_some()
            || owner.members.is_empty()
            || owner.retired.iter().any(|retired| !retired.joined)
            || !owner
                .settings
                .as_ref()
                .is_some_and(|settings| settings.enabled)
            || owner.retry_at.is_some_and(|deadline| now < deadline)
        {
            return None;
        }
        let generation = owner.generation;
        owner.starting = Some(generation);
        owner.retry_at = Some(now + START_RETRY);
        Some(generation)
    }

    pub(super) fn complete_start(&self, generation: u64, result: Result<BrainHost, String>) {
        let mut owner = self.0.borrow_mut();
        if owner.starting != Some(generation) {
            return;
        }
        owner.starting = None;
        if owner.generation != generation
            || owner.members.is_empty()
            || !owner.settings.as_ref().is_some_and(|s| s.enabled)
        {
            if let Ok(host) = result {
                owner.retire(host, generation);
            }
            return;
        }
        match result {
            Ok(host) => owner.host = Some(host),
            Err(reason) => tracing::warn!(%reason, "全局幕僚未启动"),
        }
    }
}

impl Owner {
    fn update_roots(&self) {
        let mut roots: Vec<_> = self
            .members
            .values()
            .map(|member| member.root.clone())
            .collect();
        roots.sort();
        roots.dedup();
        self.read_roots.replace(roots);
    }

    fn invalidate(&mut self) {
        let generation = self.generation;
        self.generation += 1;
        // Keep an outstanding start ticket until its worker returns. Otherwise a new
        // project could start a second process while the obsolete launch is in flight.
        self.retry_at = None;
        if let Some(host) = self.host.take() {
            self.retire(host, generation);
        }
    }

    fn retire(&mut self, mut host: BrainHost, generation: u64) {
        let (done, stopped) = mpsc::channel();
        let (_, empty) = mpsc::channel();
        self.retired.push(Retired {
            generation,
            status: std::mem::replace(&mut host.status, empty),
            stopped,
            joined: false,
        });
        if let Some(service) = host.service.take() {
            std::thread::spawn(move || {
                service.shutdown();
                let _ = done.send(());
            });
        } else {
            let _ = done.send(());
        }
    }

    fn poll(&mut self) {
        let mut received: Vec<_> = self
            .host
            .as_ref()
            .into_iter()
            .flat_map(|host| host.status.try_iter())
            .map(|status| (self.generation, status))
            .collect();
        self.retired.retain_mut(|retired| {
            if !retired.joined {
                retired.joined = !matches!(retired.stopped.try_recv(), Err(TryRecvError::Empty));
            }
            let disconnected = loop {
                match retired.status.try_recv() {
                    Ok(status) => received.push((retired.generation, status)),
                    Err(TryRecvError::Empty) => break false,
                    Err(TryRecvError::Disconnected) => break true,
                }
            };
            !retired.joined || !disconnected
        });
        for (generation, status) in received {
            if let Some(member) = self.members.get(&status.scope) {
                let _ = member.status.send(StatusEnvelope { generation, status });
            }
        }
    }
}

impl BrainClient {
    pub(super) fn tasks(&self) -> Option<std::sync::Arc<brain::staff::TaskStore>> {
        let owner = self.owner.0.borrow();
        if !self.live(&owner) {
            return None;
        }
        Some(owner.host.as_ref()?.service.as_ref()?.tasks())
    }

    fn live(&self, owner: &Owner) -> bool {
        owner
            .members
            .get(&self.scope)
            .is_some_and(|member| member.id == self.id)
    }

    pub(super) fn matches_scope(&self, scope: &BrainScope) -> bool {
        self.scope == *scope
    }

    pub(super) fn generation(&self) -> Option<u64> {
        let owner = self.owner.0.borrow();
        (self.live(&owner) && owner.host.is_some()).then_some(owner.generation)
    }

    pub(super) fn is_running(&self) -> bool {
        self.generation().is_some()
    }

    pub(super) fn token(&self) -> Option<String> {
        let owner = self.owner.0.borrow();
        self.live(&owner)
            .then(|| owner.host.as_ref().map(|host| host.token.clone()))
            .flatten()
    }

    /// Return an undelivered event for the project queue while its service is unavailable.
    pub(super) fn push(&self, event: BrainEvent) -> Option<BrainEvent> {
        let owner = self.owner.0.borrow();
        if self.live(&owner) && self.scope == event.scope {
            if let Some(service) = owner.host.as_ref().and_then(|host| host.service.as_ref()) {
                service.push(event);
                return None;
            }
        }
        Some(event)
    }

    pub(super) fn push_recorded(
        &self,
        event: BrainEvent,
        recorded: brain::staff::TaskEventResult,
    ) -> Option<(BrainEvent, brain::staff::TaskEventResult)> {
        let owner = self.owner.0.borrow();
        if self.live(&owner) && self.scope == event.scope {
            if let Some(service) = owner.host.as_ref().and_then(|host| host.service.as_ref()) {
                service.push_recorded(event, recorded);
                return None;
            }
        }
        Some((event, recorded))
    }

    pub(super) fn try_recv(&self) -> Option<ClientStatus> {
        let mut owner = self.owner.0.borrow_mut();
        owner.poll();
        if !self.live(&owner) {
            return None;
        }
        self.status.try_recv().ok().map(|entry| ClientStatus {
            status: entry.status,
            retired: entry.generation != owner.generation,
        })
    }
}

impl Drop for BrainClient {
    fn drop(&mut self) {
        let mut owner = self.owner.0.borrow_mut();
        if self.live(&owner) {
            owner.members.remove(&self.scope);
            owner.update_roots();
            if let Some(service) = owner.host.as_ref().and_then(|host| host.service.as_ref()) {
                service.release_scope(&self.scope);
            }
            if owner.members.is_empty() {
                owner.invalidate();
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_owner_tests.rs"]
mod tests;

pub(super) fn shared(cx: &mut gpui::App) -> SharedBrain {
    cx.default_global::<SharedBrain>().clone()
}

#[cfg(test)]
#[path = "../../../tests/support/brain_host.rs"]
pub(super) mod support;
