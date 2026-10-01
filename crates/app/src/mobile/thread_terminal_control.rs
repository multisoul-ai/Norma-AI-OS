use super::{
    command, server::Shared, terminal, thread_task_terminal as original, LiveTerminalTask,
};
use runtime::TerminalInputLease;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ThreadTerminalOperation {
    Claim {
        operation_id: String,
    },
    Input {
        operation_id: String,
        claim_operation_id: String,
        expected: norma::mobile::CardSnapshot,
        screen_signature: u64,
        input_sequence: u64,
        input: norma::mobile::TerminalInput,
    },
    Release {
        operation_id: String,
        claim_operation_id: String,
    },
    Read {
        claim_operation_id: String,
    },
}

#[derive(Clone, Serialize)]
pub struct ThreadTerminalCommand {
    pub origin: original::Request,
    pub target: LiveTerminalTask,
    pub operation: ThreadTerminalOperation,
    #[serde(skip)]
    pub lease: Arc<TerminalInputLease>,
}

impl ThreadTerminalOperation {
    fn identifiers(&self) -> (Option<&str>, &str) {
        match self {
            Self::Claim { operation_id } => (Some(operation_id), operation_id),
            Self::Input {
                operation_id,
                claim_operation_id,
                ..
            }
            | Self::Release {
                operation_id,
                claim_operation_id,
            } => (Some(operation_id), claim_operation_id),
            Self::Read { claim_operation_id } => (None, claim_operation_id),
        }
    }
}

#[derive(Default)]
pub(super) struct Connection {
    pub scope: Arc<Scope>,
    binding: Option<ThreadTerminalCommand>,
}

/// The network future keeps a cancellation guard even while a blocking handler owns the Peer.
#[derive(Default)]
pub(super) struct Scope(Mutex<(bool, Option<Arc<TerminalInputLease>>)>);

impl Scope {
    fn install(&self, lease: &Arc<TerminalInputLease>) -> Result<(), &'static str> {
        let mut state = self.0.lock().map_err(|_| "unavailable")?;
        if state.0 {
            return Err("terminal_control_expired");
        }
        if let Some(previous) = state.1.replace(lease.clone()) {
            previous.revoke();
        }
        Ok(())
    }

    pub fn close(&self) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        state.0 = true;
        if let Some(lease) = state.1.take() {
            lease.revoke();
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.scope.close();
    }
}

pub(super) struct PendingDisconnect(pub Option<Arc<Scope>>);
impl Drop for PendingDisconnect {
    fn drop(&mut self) {
        if let Some(scope) = &self.0 {
            scope.close();
        }
    }
}

pub(super) fn handle(
    shared: &Arc<Shared>,
    machine: &str,
    device: &str,
    token: &str,
    origin: &original::Request,
    operation: &ThreadTerminalOperation,
    lookup_only: bool,
    connection: &mut Connection,
) -> Result<Value, &'static str> {
    let (operation_id, claim) = operation.identifiers();
    if !command::valid_id(claim)
        || operation_id.is_some_and(|id| !command::valid_id(id))
        || (lookup_only && operation_id.is_none())
        || (operation_id.is_some() && (origin.before.is_some() || origin.revision.is_some()))
        || matches!(operation, ThreadTerminalOperation::Input { input, .. } if !input.valid())
    {
        return Err("invalid_request");
    }
    let (scope, target, revision) = original::resolve(shared, machine, device, token, origin)?;
    let lease = {
        // Register under the same authorization owner used by permission edits, before dispatch.
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(machine, device, token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        if grant.revision != revision || !grant.allows(&scope.workspace_id, true) {
            return Err("unauthorized");
        }
        let existing = if let Some(id) = operation_id {
            storage::MobileCommandStore::new(access.store.path().to_path_buf())
                .get(device, id, &scope.workspace_id, &shared.owner_id)
                .map_err(|_| "unavailable")?
                .is_some()
        } else {
            false
        };
        if lookup_only && !existing {
            return Err("not_found");
        }
        let current = connection.binding.as_ref().filter(|binding| {
            binding.operation.identifiers().1 == claim
                && binding.target == target
                && binding.origin.collaboration_workspace_id == origin.collaboration_workspace_id
                && binding.origin.thread_id == origin.thread_id
                && binding.origin.operation_id == origin.operation_id
        });
        let lease = if existing {
            // Durable lookup never creates a new owner. A disconnected retry can only recover its receipt.
            if let Some(binding) = current {
                binding.lease.clone()
            } else {
                let lease = TerminalInputLease::new(target.key.clone())?;
                lease.revoke();
                lease
            }
        } else if matches!(operation, ThreadTerminalOperation::Claim { .. }) {
            if connection
                .binding
                .as_ref()
                .is_some_and(|binding| !binding.lease.is_revoked())
            {
                return Err("terminal_controlled");
            }
            let lease = shared.terminal_lease(device, target.key.clone())?;
            connection.scope.install(&lease)?;
            connection.binding = Some(ThreadTerminalCommand {
                origin: origin.clone(),
                target: target.clone(),
                operation: operation.clone(),
                lease: lease.clone(),
            });
            lease
        } else {
            current
                .filter(|binding| !binding.lease.is_revoked())
                .ok_or("terminal_control_expired")?
                .lease
                .clone()
        };
        lease
    };
    if let Some(id) = operation_id {
        let result = command::submit_or_lookup(
            shared,
            machine,
            device,
            token,
            super::MobileCommandRequest {
                operation_id: id.into(),
                workspace_id: scope.workspace_id,
                route_id: scope.route_id,
                grant_revision: revision,
                command: super::MobileCommand::ThreadTerminal {
                    command: Box::new(ThreadTerminalCommand {
                        origin: origin.clone(),
                        target,
                        operation: operation.clone(),
                        lease: lease.clone(),
                    }),
                },
            },
            lookup_only,
        );
        if matches!(operation, ThreadTerminalOperation::Claim { .. })
            && (result.is_err()
                || result.as_ref().is_ok_and(|reply| {
                    matches!(
                        reply["receipt"]["state"].as_str(),
                        Some("rejected" | "outcome_unknown")
                    )
                }))
        {
            lease.revoke();
        }
        result
    } else {
        let page =
            terminal::read_controlled(shared, scope.clone(), Some(target.clone()), Some(lease))?;
        original::reauthorize(shared, machine, device, token, revision)?;
        original::response(origin, &scope, &target, page)
    }
}
