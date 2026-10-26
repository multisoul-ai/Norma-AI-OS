//! Wrap memory and desktop adapters together, outside the tool authorization boundary.
use agent::{AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, ToolContext};
use futures::future::BoxFuture;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use storage::{
    LiveOperationRequest, LiveOperationReservation, LiveOperationState, LiveOperationStore,
};

type Outcome = Result<AdapterResult, AdapterError>;

pub(super) struct LiveDurableAdapter {
    delegate: Arc<dyn AgentCardAdapter>,
    store: LiveOperationStore,
    scope: Scope,
    receipts: Arc<Mutex<Vec<Outcome>>>,
    pending: Arc<AtomicUsize>,
    effects: AtomicBool,
}

#[derive(Clone)]
struct Scope {
    project: String,
    intent: String,
    owner: String,
}

struct PendingExecution(Arc<AtomicUsize>);
impl Drop for PendingExecution {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl LiveDurableAdapter {
    pub(super) fn new(
        delegate: Arc<dyn AgentCardAdapter>,
        database: PathBuf,
        project: String,
        intent: String,
        owner: String,
    ) -> Self {
        Self {
            delegate,
            store: LiveOperationStore::new(database),
            scope: Scope {
                project,
                intent,
                owner,
            },
            receipts: Arc::new(Mutex::new(Vec::new())),
            pending: Arc::new(AtomicUsize::new(0)),
            effects: AtomicBool::new(false),
        }
    }

    pub(super) fn receipts(&self) -> Vec<Outcome> {
        self.receipts.lock().expect("Live receipts lock").clone()
    }

    pub(super) fn is_pending(&self) -> bool {
        self.pending.load(Ordering::Acquire) > 0
    }

    pub(super) fn has_effects(&self) -> bool {
        self.effects.load(Ordering::Acquire)
    }

    /// Read only after planning has ended: pending can then decrease, but no new work can start.
    pub(super) fn completion_snapshot(&self) -> (bool, Vec<Outcome>) {
        let pending = self.is_pending();
        (pending, self.receipts())
    }
}

impl AgentCardAdapter for LiveDurableAdapter {
    fn shared_context<'a>(
        &'a self,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<Option<agent::context::ContextFrame>, AdapterError>> {
        self.delegate.shared_context(context)
    }
    fn memory_context(
        &self,
    ) -> BoxFuture<'_, Result<Option<agent::MemorySessionContext>, AdapterError>> {
        self.delegate.memory_context()
    }

    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Outcome> {
        Box::pin(async move {
            if context.input_source != agent::AgentInputSource::GptLiveVoice
                || context.project_id != self.scope.project
            {
                return Err(AdapterError::Rejected(
                    "GPT-Live工具来源或项目不匹配".into(),
                ));
            }
            if context.cancellation.is_cancelled() {
                return Err(AdapterError::Cancelled);
            }
            if command.is_read_only() {
                let result = self.delegate.execute(command, context).await;
                let mut observed = self.receipts.lock().expect("Live receipts lock");
                if observed.len() < 64 {
                    observed.push(result.clone());
                }
                return result;
            }
            let operation = operation_step(context)?;
            self.effects.store(true, Ordering::Release);
            let (delegate, store, scope, receipts, context) = (
                self.delegate.clone(),
                self.store.clone(),
                self.scope.clone(),
                self.receipts.clone(),
                context.clone(),
            );
            self.pending.fetch_add(1, Ordering::AcqRel);
            let pending = PendingExecution(self.pending.clone());
            // This task owns the receipt once work is dispatched; a dropped planning future cannot drop it.
            tokio::spawn(async move {
                let _pending = pending;
                let result = execute(delegate, store, scope, operation, command, context).await;
                let mut observed = receipts.lock().expect("Live receipts lock");
                if observed.len() < 64 {
                    observed.push(result.clone());
                }
                result
            })
            .await
            .unwrap_or_else(|_| unknown("执行线程中断，结果待核对；不会自动重做"))
        })
    }
}

fn operation_step(context: &ToolContext) -> Result<String, AdapterError> {
    let suffix = context
        .operation_id
        .as_deref()
        .and_then(|id| id.strip_prefix(&format!("{}:", context.turn_id)))
        .ok_or_else(|| AdapterError::Rejected("Live工具缺少宿主操作序号".into()))?;
    let step = if let Some(step) = suffix.strip_prefix("step-") {
        step.parse::<u32>().ok().filter(|step| *step > 0)
    } else {
        suffix
            .strip_prefix("direct-assign-")
            .or_else(|| suffix.strip_prefix("direct-reply-"))
            .and_then(|step| step.parse::<u32>().ok())
            .and_then(|step| step.checked_add(1))
    }
    .ok_or_else(|| AdapterError::Rejected("Live工具操作序号无效".into()))?;
    // Delegation IDs, revisions and provider call IDs never create a second execution identity.
    Ok(format!("step-{step}"))
}

async fn execute(
    delegate: Arc<dyn AgentCardAdapter>,
    store: LiveOperationStore,
    scope: Scope,
    operation: String,
    command: AdapterCommand,
    context: ToolContext,
) -> Outcome {
    let reservation = tokio::task::spawn_blocking({
        let (store, command) = (store.clone(), command.clone());
        move || {
            store.reserve(&LiveOperationRequest {
                project_id: &scope.project,
                intent_id: &scope.intent,
                operation_id: &operation,
                owner_id: &scope.owner,
                command: &command,
            })
        }
    })
    .await;
    let lease = match reservation {
        Ok(Ok(LiveOperationReservation::Claimed(lease))) => lease,
        Ok(Ok(LiveOperationReservation::Existing(receipt))) => {
            return if receipt.state == LiveOperationState::Recorded {
                receipt
                    .outcome
                    .unwrap_or_else(|| unknown("持久回执缺失，待核对"))
            } else {
                unknown("原操作未确认，须先核对；不会重复执行")
            };
        }
        _ => return unknown("操作账本不可用或参数冲突，未再次执行；请核对"),
    };
    let result = if context.cancellation.is_cancelled() {
        Err(AdapterError::Cancelled)
    } else {
        delegate.execute(command, &context).await
    };
    let persisted = tokio::task::spawn_blocking({
        let result = result.clone();
        move || store.record(&lease, &result)
    })
    .await;
    if !matches!(persisted, Ok(Ok(()))) {
        return unknown("操作回执未能持久保存，结果待核对；不会自动重做");
    }
    match result {
        Ok(result) if matches!(result.status.as_str(), "accepted" | "succeeded") => Ok(result),
        _ => unknown("执行结果未确认，须先核对；已停止后续工具，不会自动重做"),
    }
}

fn unknown(message: &str) -> Outcome {
    Ok(AdapterResult::unknown(message))
}

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_durable_adapter_tests.rs"]
mod tests;
