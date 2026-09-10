use super::*;
use crate::mobile::{CommandLease, LiveCommandRequest, LiveSnapshotRequest, MobileCommand};
use crossbeam_channel::{Receiver, TryRecvError};
use norma::mobile::ProjectSnapshot;
use std::{
    collections::HashSet,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

struct Sending {
    entry: QueuedMessage,
    lease: Arc<CommandLease>,
    receipt: Receiver<Result<Value, &'static str>>,
    deadline: Instant,
}

pub(in crate::mobile) fn start(shared: Arc<Shared>) -> anyhow::Result<()> {
    if shared.commands.is_none() || shared.snapshots.is_none() {
        return Ok(());
    }
    std::thread::Builder::new().name("norma-mobile-queue".into()).spawn(move || {
        if let Err(error) = run(&shared) {
            if !shared.stopped.load(Ordering::Acquire) {
                tracing::warn!(%error, "mobile queue stopped without replaying uncertain deliveries");
                shared.stop();
            }
        }
    })?;
    Ok(())
}

fn run(shared: &Shared) -> Result<(), String> {
    let store = {
        let access = shared.access().map_err(|e| e.to_string())?;
        MobileCommandStore::new(access.store.path().to_path_buf())
    };
    let mut sending = Vec::new();
    while !shared.stopped.load(Ordering::Acquire) {
        poll_receipts(&store, &mut sending)?;
        let entries = store
            .queued_messages(&shared.owner_id)
            .map_err(|e| e.to_string())?;
        if !entries.is_empty() && sending.len() < 8 {
            let grants = {
                let access = shared.access().map_err(|e| e.to_string())?;
                access.devices().map_err(|e| e.to_string())?
            };
            let mut authorized = Vec::new();
            for entry in entries {
                if grants.iter().any(|g| {
                    g.device_id == entry.request.device_id
                        && g.revision == entry.message.grant_revision
                        && g.allows(&entry.request.workspace_id, true)
                }) {
                    authorized.push(entry);
                } else {
                    store
                        .cancel_queued_message(&entry.request)
                        .map_err(|e| e.to_string())?;
                    crate::mobile::project_events::changed();
                }
            }
            if let Some(projects) = snapshots(shared, &authorized) {
                for entry in authorized {
                    if sending.len() >= 8 {
                        break;
                    }
                    let Some(project) = projects
                        .iter()
                        .find(|p| p.workspace_id == entry.request.workspace_id)
                    else {
                        continue;
                    };
                    let card = project.cards.as_ref().and_then(|cards| {
                        cards.iter().find(|c| c.node_id == entry.message.node_id)
                    });
                    if project.route_id.as_deref() != Some(&entry.request.route_id)
                        || card.is_none_or(|card| {
                            !card.mobile_control
                                || card.profile != entry.message.profile
                                || card.run_id.as_deref() != Some(&entry.message.run_id)
                                || card.session_id.as_deref() != Some(&entry.message.session_id)
                        })
                    {
                        store
                            .cancel_queued_message(&entry.request)
                            .map_err(|e| e.to_string())?;
                        crate::mobile::project_events::changed();
                        continue;
                    }
                    let card = card.expect("validated live card");
                    if !matches!(card.state.as_str(), "ready" | "done" | "ended" | "failed") {
                        continue;
                    }
                    if let Some(pending) = dispatch(shared, &store, entry, card.clone())? {
                        sending.push(pending);
                    }
                }
            }
        }
        // ponytail: one Mac-owned worker checks at 2Hz, at most 128 waiting / 8 sending.
        // Replace idle polling with an enqueue signal only if measured wakeups warrant it.
        std::thread::sleep(Duration::from_millis(500));
    }
    // No foreground join or write on shutdown. A new owner projects unconfirmed sends as unknown.
    Ok(())
}

fn snapshots(shared: &Shared, entries: &[QueuedMessage]) -> Option<Vec<ProjectSnapshot>> {
    if entries.is_empty() {
        return Some(vec![]);
    }
    let mut seen = HashSet::new();
    let projects = entries
        .iter()
        .filter(|entry| seen.insert(entry.request.workspace_id.clone()))
        .map(|entry| ProjectSnapshot {
            workspace_id: entry.request.workspace_id.clone(),
            display_name: "Queue".into(),
            state: "unavailable".into(),
            route_id: None,
            card_count: None,
            cards: None,
        })
        .collect();
    let (reply, result) = crossbeam_channel::bounded(1);
    // Same 2s foreground snapshot budget as a normal phone read; transient loss causes no claim.
    let deadline = Instant::now() + Duration::from_secs(2);
    shared
        .snapshots
        .as_ref()?
        .try_send(LiveSnapshotRequest {
            projects,
            include_cards: true,
            deadline,
            reply,
        })
        .ok()?;
    result
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()
}

fn dispatch(
    shared: &Shared,
    store: &MobileCommandStore,
    entry: QueuedMessage,
    card: CardSnapshot,
) -> Result<Option<Sending>, String> {
    // Permission edits use the same lock and cancel all unstarted leases before releasing it.
    let access = shared.access().map_err(|e| e.to_string())?;
    let allowed = access
        .devices()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|g| {
            g.device_id == entry.request.device_id
                && g.revision == entry.message.grant_revision
                && g.allows(&entry.request.workspace_id, true)
        });
    if !allowed {
        store
            .cancel_queued_message(&entry.request)
            .map_err(|e| e.to_string())?;
        crate::mobile::project_events::changed();
        return Ok(None);
    }
    let lease = match shared.command_lease(&entry.request.device_id, &entry.request.operation_id) {
        Ok(lease) => lease,
        Err("busy") => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !store
        .claim_queued_message(&entry.request)
        .map_err(|e| e.to_string())?
    {
        return Ok(None);
    }
    crate::mobile::project_events::changed();
    let (reply, receipt) = crossbeam_channel::bounded(1);
    //10s baseline +25s Provider acknowledgement +5s receipt margin; queue waiting remains unbounded.
    let deadline = Instant::now() + Duration::from_secs(40);
    let request = MobileCommandRequest {
        operation_id: entry.request.operation_id.clone(),
        workspace_id: entry.request.workspace_id.clone(),
        route_id: entry.request.route_id.clone(),
        grant_revision: entry.message.grant_revision,
        command: MobileCommand::SendMessage {
            expected: card,
            text: entry.message.text.clone(),
        },
    };
    if shared
        .commands
        .as_ref()
        .ok_or("queue dispatcher unavailable")?
        .try_send(LiveCommandRequest {
            source: entry.request.clone(),
            request,
            lease: lease.clone(),
            deadline,
            reply,
        })
        .is_err()
    {
        // try_send failure proves the App never received this attempt.
        lease.cancel();
        lease.finished.store(true, Ordering::Release);
        store
            .defer_queued_message(&entry.request)
            .map_err(|e| e.to_string())?;
        crate::mobile::project_events::changed();
        return Ok(None);
    }
    Ok(Some(Sending {
        entry,
        lease,
        receipt,
        deadline,
    }))
}

fn poll_receipts(store: &MobileCommandStore, sending: &mut Vec<Sending>) -> Result<(), String> {
    let mut index = 0;
    while index < sending.len() {
        let item = &sending[index];
        let result = match item.receipt.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Disconnected) => Some(Err("outcome_unknown")),
            Err(TryRecvError::Empty) if Instant::now() >= item.deadline => {
                Some(Err(if item.lease.cancel() {
                    "expired"
                } else {
                    "outcome_unknown"
                }))
            }
            Err(TryRecvError::Empty) => None,
        };
        let Some(result) = result else {
            index += 1;
            continue;
        };
        match result {
            // These stable codes originate only from the App's validation before any Provider input.
            Err(
                "state_not_ready" | "desktop_input_pending" | "terminal_not_ready" | "card_changed",
            ) => store
                .defer_queued_message(&item.entry.request)
                .map_err(|e| e.to_string())?,
            Ok(value) => store
                .finish_queued_message(&item.entry.request, Ok(value))
                .map_err(|e| e.to_string())?,
            Err(code) => {
                let error = match code {
                    "route_expired"
                    | "unauthorized"
                    | "mobile_control_required"
                    | "not_found"
                    | "expired"
                    | "unavailable" => code,
                    _ => "outcome_unknown",
                };
                store
                    .finish_queued_message(&item.entry.request, Err(error.into()))
                    .map_err(|e| e.to_string())?;
            }
        }
        crate::mobile::project_events::changed();
        item.lease.finished.store(true, Ordering::Release);
        sending.remove(index);
    }
    Ok(())
}
