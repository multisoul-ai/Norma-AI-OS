//! Canvas-bound host service. IO stays in spawn_blocking; the foreground gate never reads SQLite.
mod action;
pub(crate) use action::brain_action_context_failure;
pub(crate) mod adapter;
mod coordination;
pub(crate) mod report;
mod scope;
use agent::{context::*, AdapterError};
use std::sync::{Arc, Mutex};
use storage::{ContextAccess, ContextStore, StorageResult};

#[derive(Clone)]
pub(crate) struct SharedContext {
    access: ContextAccess,
    coordination: Arc<coordination::Coordination>,
}

impl SharedContext {
    pub(crate) fn new(path: std::path::PathBuf, scope: ContextScope) -> Result<Self, AdapterError> {
        let access =
            ContextAccess::new(ContextStore::new(path.clone()), scope.clone()).map_err(failed)?;
        Ok(Self {
            access,
            coordination: coordination::for_scope(path, scope)?,
        })
    }

    pub(crate) fn scope(&self) -> &ContextScope {
        self.access.scope()
    }

    /// The host owns accepted work and recovery, independent of a model call ID or waiter.
    /// Critical closures must perform idempotent storage operations only.
    /// Changes to effective rules require a mutation identity. Noncritical work may
    /// read or append source facts, but must never update/revoke effective rules.
    pub(crate) async fn run<T: Send + 'static>(
        &self,
        mutation: Option<String>,
        work: impl Fn(&ContextAccess) -> StorageResult<T> + Send + Sync + 'static,
    ) -> Result<T, AdapterError> {
        let service = self.clone();
        super::agent_runtime::shared_tokio_runtime()
            .map_err(AdapterError::Failed)?
            .spawn(async move { service.run_owned(mutation.is_some(), work).await })
            .await
            .map_err(failed)?
    }

    async fn run_owned<T: Send + 'static>(
        &self,
        critical: bool,
        work: impl Fn(&ContextAccess) -> StorageResult<T> + Send + Sync + 'static,
    ) -> Result<T, AdapterError> {
        let mut pending = self.coordination.serial.lock().await;
        if !self
            .coordination
            .initialized
            .load(std::sync::atomic::Ordering::Acquire)
        {
            // Active delivery writers retain this coordinator. Only a new, unowned
            // scope can recover orphaned reports; another consumer cannot reset playback.
            let (result, snapshot) = self
                .background(|access| access.recover_reports(&access.snapshot()?))
                .await?;
            result.map_err(failed)?;
            snapshot.map_err(failed)?;
            self.coordination
                .initialized
                .store(true, std::sync::atomic::Ordering::Release);
        }
        // Recover before recording a new input or any other fact can advance the CAS revision.
        if let Some(recover) = pending.clone() {
            let (result, snapshot) = self.background(move |access| recover(access)).await?;
            if settled(&result) {
                *pending = None;
            }
            self.install(snapshot, pending.is_some())?;
            result.map_err(failed)?;
        }
        let work = Arc::new(work);
        if critical {
            let retry = work.clone();
            *pending = Some(Arc::new(move |access| retry(access).map(|_| ())));
            self.coordination
                .pending
                .store(true, std::sync::atomic::Ordering::Release);
            self.coordination.gate.lock().map_err(failed)?.begin_write();
        }
        let (result, snapshot) = self.background(move |access| work(access)).await?;
        if critical && settled(&result) {
            *pending = None;
        }
        self.install(snapshot, pending.is_some())?;
        result.map_err(failed)
    }

    async fn background<T: Send + 'static>(
        &self,
        work: impl FnOnce(&ContextAccess) -> StorageResult<T> + Send + 'static,
    ) -> Result<(StorageResult<T>, StorageResult<ContextSnapshot>), AdapterError> {
        let access = self.access.clone();
        super::agent_runtime::shared_tokio_runtime()
            .map_err(AdapterError::Failed)?
            .spawn_blocking(move || (work(&access), access.snapshot()))
            .await
            .map_err(failed)
    }

    fn install(
        &self,
        snapshot: StorageResult<ContextSnapshot>,
        pending: bool,
    ) -> Result<(), AdapterError> {
        let mut gate = self.coordination.gate.lock().map_err(failed)?;
        self.coordination
            .pending
            .store(pending, std::sync::atomic::Ordering::Release);
        if pending {
            gate.begin_write();
            return Ok(());
        }
        match snapshot {
            Ok(snapshot) => gate.install(snapshot).map_err(failed),
            Err(error) => {
                gate.begin_write();
                Err(failed(error))
            }
        }
    }

    pub(crate) async fn snapshot(&self) -> Result<ContextSnapshot, AdapterError> {
        self.run(None, ContextAccess::snapshot).await
    }

    /// Hold the same lock until the synchronous host side effect has been accepted.
    /// A background state write cannot begin between this check and that acceptance.
    pub(crate) fn execute<T>(
        &self,
        operation: ContextOperation,
        target: Option<&str>,
        action: impl FnOnce() -> Result<T, AdapterError>,
    ) -> Result<T, AdapterError> {
        let gate = self.coordination.gate.lock().map_err(failed)?;
        gate.check(operation, target)
            .map_err(|e| AdapterError::Rejected(e.into()))?;
        action()
    }
}

fn settled<T>(result: &StorageResult<T>) -> bool {
    result.is_ok() || matches!(result, Err(storage::StorageError::InvalidContext(_)))
}

fn failed(error: impl std::fmt::Display) -> AdapterError {
    AdapterError::Failed(error.to_string())
}

#[cfg(test)]
#[path = "../../tests/unit/shared_context_tests.rs"]
mod tests;
