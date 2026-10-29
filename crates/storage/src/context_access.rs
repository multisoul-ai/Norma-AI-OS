//! Scope-bound IO seam. Construct only from the host's authenticated canvas identity.
//! All methods block and must run on a background executor.
use crate::{ContextStore, StorageError, StorageResult};
use agent::context::*;
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct ContextAccess {
    store: ContextStore,
    scope: ContextScope,
}

impl ContextAccess {
    pub fn new(store: ContextStore, scope: ContextScope) -> StorageResult<Self> {
        if !scope.is_valid() {
            return Err(invalid("invalid_scope"));
        }
        Ok(Self { store, scope })
    }

    pub fn scope(&self) -> &ContextScope {
        &self.scope
    }

    pub fn snapshot(&self) -> StorageResult<ContextSnapshot> {
        self.store.snapshot(&self.scope, 100)
    }

    pub fn read(&self, reference: &ContextRef) -> StorageResult<RecordedContextEvent> {
        self.store.read(&self.scope, reference)
    }

    /// Called once by a new scope host, before accepting any live delivery work.
    /// No audio or operations are replayed. CAS prevents overwriting a concurrent receipt.
    /// Partial recovery is safe to retry: committed Unknown facts are skipped.
    pub fn recover_reports(&self, snapshot: &ContextSnapshot) -> StorageResult<()> {
        if snapshot.scope != self.scope {
            return Err(invalid("scope_mismatch"));
        }
        let mut revision = snapshot.revision;
        for report in snapshot.state.reports.values() {
            if report_delivery_is_terminal(report.delivery) {
                continue;
            }
            let record = self.write(
                &format!("host:recover:{}", report.reference.event_id),
                ContextOrigin::Host,
                ContextPayload::Delivery {
                    report: report.reference.clone(),
                    status: ReportDelivery::Unknown,
                },
                vec![report.reference.clone()],
                Some(revision),
            )?;
            revision = record.reference.revision;
        }
        Ok(())
    }

    pub fn record_input(
        &self,
        turn: &str,
        origin: ContextOrigin,
        text: &str,
    ) -> StorageResult<RecordedContextEvent> {
        if matches!(origin, ContextOrigin::Host | ContextOrigin::Brain) {
            return Err(invalid("input_requires_user_source"));
        }
        self.write(
            &format!("input:{turn}"),
            origin,
            ContextPayload::UserInput { text: text.into() },
            vec![],
            None,
        )
    }

    /// The current input reference is supplied by the host, never taken from tool arguments.
    pub fn update(
        &self,
        operation: &str,
        current_input: &ContextRef,
        update: &ContextStateUpdate,
    ) -> StorageResult<RecordedContextEvent> {
        if current_input != &update.input || current_input.scope != self.scope {
            return Err(invalid("state_source_mismatch"));
        }
        let input = self.read(current_input)?;
        let payload = validate_state_update(&input, update).map_err(invalid)?;
        self.write(
            &format!("state:{operation}"),
            input.event.source.origin,
            payload,
            vec![current_input.clone()],
            Some(update.expected_revision),
        )
    }

    pub fn report(
        &self,
        id: &str,
        text: &str,
        sources: Vec<ContextRef>,
    ) -> StorageResult<RecordedContextEvent> {
        self.write(
            &format!("report:{id}"),
            ContextOrigin::Brain,
            ContextPayload::Report { text: text.into() },
            sources,
            None,
        )
    }

    /// Host receipts/delivery are not model state proposals. This cannot write user rules.
    pub fn host_event(
        &self,
        id: &str,
        payload: ContextPayload,
        related: Vec<ContextRef>,
    ) -> StorageResult<RecordedContextEvent> {
        if !matches!(
            payload,
            ContextPayload::Receipt { .. }
                | ContextPayload::Decision { .. }
                | ContextPayload::Delivery { .. }
        ) {
            return Err(invalid("invalid_host_fact"));
        }
        self.write(
            &format!("host:{id}"),
            ContextOrigin::Host,
            payload,
            related,
            None,
        )
    }

    fn write(
        &self,
        key: &str,
        origin: ContextOrigin,
        payload: ContextPayload,
        related: Vec<ContextRef>,
        expected: Option<u64>,
    ) -> StorageResult<RecordedContextEvent> {
        if key.len() > 1024
            || key
                .split_once(':')
                .is_none_or(|(_, id)| id.trim().is_empty())
        {
            return Err(invalid("invalid_operation_identity"));
        }
        let encoded = serde_json::to_vec(&(&self.scope, origin, key))?;
        let id = format!("ctx-{:x}", Sha256::digest(encoded));
        let validate_retry = |record: RecordedContextEvent| {
            if record.event.payload == payload
                && record.event.related == related
                && record.event.source.origin == origin
            {
                Ok(record)
            } else {
                Err(invalid("idempotency_conflict"))
            }
        };
        if let Some(record) = self.store.find(&self.scope, &id)? {
            return validate_retry(record);
        }
        let event = ContextEvent {
            event_id: id.clone(),
            scope: self.scope.clone(),
            // Each operation has one immutable fact; retries reuse the same stream identity.
            source: ContextSource {
                origin,
                stream_id: id.clone(),
                sequence: 1,
            },
            created_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| invalid("clock_before_epoch"))?
                .as_millis()
                .min(i64::MAX as u128) as i64,
            related: related.clone(),
            payload: payload.clone(),
        };
        match self.store.append(&event, expected) {
            Ok(record) => Ok(record),
            Err(error) => match self.store.find(&self.scope, &id)? {
                // Another caller may have committed the identical operation in between.
                Some(record) => validate_retry(record),
                None => Err(error),
            },
        }
    }
}

fn invalid(code: &str) -> StorageError {
    StorageError::InvalidContext(code.into())
}
