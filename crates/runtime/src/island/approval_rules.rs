//! An explicit remembered approval authorizes one exact operation on one card.
use norma::{agent_hook::ApprovalDecision, mobile::ApprovalSnapshot};
use sha2::{Digest, Sha256};

pub fn approval_rule_key(
    workspace: &str,
    area: &str,
    root: &str,
    approval: &ApprovalSnapshot,
) -> Option<String> {
    if !approval.target.valid()
        || workspace.is_empty()
        || area.is_empty()
        || root.is_empty()
        || approval.tool.is_empty()
        || !approval.details.is_object()
        || !approval.decisions.contains(&ApprovalDecision::Allow)
    {
        return None;
    }
    let mut input = approval.details.clone();
    if approval.target.profile == "Codex" {
        match approval.tool.as_str() {
            "Bash" if input["command"].is_string() => {} // verified native dialog
            "item/commandExecution/requestApproval"
                if input["command"].is_string() && input["cwd"].is_string() =>
            {
                let fields = input.as_object_mut()?;
                for identity in ["threadId", "turnId", "itemId"] {
                    fields.remove(identity);
                }
            }
            // File-change requests may refer to a patch only by itemId. Never erase that scope.
            _ => return None,
        }
    }
    // Request/run/turn IDs change per invocation; card, provider, root and tool input do not.
    let scope = serde_json::to_vec(&(
        workspace,
        area,
        root,
        approval.target.node_id,
        &approval.target.profile,
        &approval.tool,
        &input,
    ))
    .ok()?;
    Some(format!(
        "island.approval.allow.v1:{:x}",
        Sha256::digest(scope)
    ))
}

#[cfg(test)]
#[path = "../../tests/unit/island_approval_rules.rs"]
mod tests;
