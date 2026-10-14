//! Receipt metadata supplements task snapshots; it never invents CLI completion or activity.
use norma::connector::{Snapshot, Task, TaskEvent};
use storage::MobileCommandReceipt;
pub(super) fn apply(snapshot: &mut Snapshot, receipts: &[MobileCommandReceipt], device: &str) {
    for receipt in receipts {
        let Some(value) = &receipt.result else {
            continue;
        };
        let Some(card_id) = value["card_id"].as_str() else {
            continue;
        };
        if value["created_by"] == "grokbot" {
            for card in snapshot.workspaces.iter_mut().flat_map(|w| &mut w.cards) {
                if card.card_id == card_id {
                    card.created_by = Some("grokbot".into());
                }
            }
        }
        let Some(task_id) = value["task_id"].as_str() else {
            continue;
        };
        if value.get("accepted_at").is_none() && value.get("created_at").is_none() {
            continue;
        }
        if !snapshot.tasks.iter().any(|t| t.task_id == task_id) {
            snapshot.tasks.push(Task {
                task_id: task_id.into(),
                card_id: card_id.into(),
                role_name: value["role_name"].as_str().unwrap_or_default().into(),
                workspace_id: receipt.workspace_id.clone(),
                state: value["task_state"]
                    .as_str()
                    .or(value["state"].as_str())
                    .filter(|s| *s != "loaded")
                    .unwrap_or("accepted")
                    .into(),
                tracking: value["tracking"].as_str().unwrap_or("tracked").into(),
                summary: String::new(),
                created_at: receipt.created_at_unix,
                updated_at: receipt.created_at_unix,
                source: "grokbot".into(),
                operation_id: None,
                progress: None,
                pending_approval: None,
                result_summary: None,
                error: None,
                state_changed_at: None,
                events: Vec::new(),
            });
        }
        if let Some(task) = snapshot.tasks.iter_mut().find(|t| t.task_id == task_id) {
            task.source = "grokbot".into();
            if receipt.device_id == device {
                task.operation_id = Some(receipt.operation_id.clone());
            }
            task.events.push(TaskEvent {
                r#type: "accepted".into(),
                occurred_at: receipt.created_at_unix,
                state: Some("accepted".into()),
            });
            task.events
                .sort_by_key(|e| std::cmp::Reverse(e.occurred_at));
            task.events.truncate(20);
        }
    }
}
