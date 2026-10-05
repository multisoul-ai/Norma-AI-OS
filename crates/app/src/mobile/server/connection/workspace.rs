use super::Action;

pub(super) fn workspace_credentials(action: &Action) -> Option<(&str, &str)> {
    match action {
        Action::WorkspaceActivity {
            device_id, token, ..
        } => Some((device_id, token)),
        Action::WorkspaceTerminal {
            device_id, token, ..
        } => Some((device_id, token)),
        Action::WorkspaceAgentCreate {
            device_id, token, ..
        }
        | Action::WorkspaceAgentCreationReceipt {
            device_id, token, ..
        }
        | Action::WorkspaceCatalog { device_id, token }
        | Action::WorkspaceMessage {
            device_id, token, ..
        }
        | Action::WorkspaceReceipt {
            device_id, token, ..
        }
        | Action::WorkspaceEvents {
            device_id, token, ..
        }
        | Action::WorkspaceConfigure {
            device_id, token, ..
        } => Some((device_id, token)),
        _ => None,
    }
}
