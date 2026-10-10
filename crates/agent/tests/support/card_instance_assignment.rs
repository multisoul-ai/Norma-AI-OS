use agent::card_instance::{CardInstanceAssignment, CardInstanceTracking};
use norma::workspace_dispatch::{CardExecutionFingerprint, WorkspaceCardRef, WorkspaceCardTarget};

pub fn assignment(
    id: &str,
    node: u64,
    card: &str,
    definition: Option<i64>,
) -> CardInstanceAssignment {
    CardInstanceAssignment {
        assignment_id: id.into(),
        target: WorkspaceCardTarget {
            card: WorkspaceCardRef {
                workspace_id: "workspace".into(),
                area_id: "area".into(),
                node_id: node,
                card_instance_id: card.into(),
            },
            execution: CardExecutionFingerprint {
                runtime_instance_id: "runtime".into(),
                route_generation: 1,
                terminal_session_id: Some(node),
                run_id: Some("run".into()),
                session_id: Some("session".into()),
            },
        },
        name_at_acceptance: "Alan".into(),
        definition_id: definition,
        text: format!("task-{id}"),
        tracking: CardInstanceTracking::Managed,
        force_untracked: false,
    }
}
