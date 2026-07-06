use super::{
    AgentDefinitionDraft, AgentDefinitionField, MAX_AGENT_NAME_CHARS,
    MAX_WORKFLOW_DESCRIPTION_CHARS,
};
use canvas::AgentProfile;
use gpui::Keystroke;
use storage::AgentDefinition;

pub(crate) const DEFAULT_AGENT_WORKFLOW: &str = "Describe this agent's workflow.";

pub(crate) fn validate_agent_definition_text(
    name: &str,
    workflow_description: &str,
) -> Result<(), String> {
    let name = name.trim();
    let workflow = workflow_description.trim();

    if name.is_empty() {
        return Err("Agent name is required".to_string());
    }
    if name.chars().count() > MAX_AGENT_NAME_CHARS {
        return Err("Agent name must be 80 characters or fewer".to_string());
    }
    if workflow.is_empty() {
        return Err("Workflow description is required".to_string());
    }
    if workflow.chars().count() > MAX_WORKFLOW_DESCRIPTION_CHARS {
        return Err("Workflow description must be 2000 characters or fewer".to_string());
    }

    Ok(())
}

pub(super) fn display_value(field: AgentDefinitionField, value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        match field {
            AgentDefinitionField::Name => "Set the agent name.".to_string(),
            AgentDefinitionField::WorkflowDescription => {
                "Describe the workflow this agent should follow.".to_string()
            }
        }
    } else {
        value.to_string()
    }
}

pub(super) fn readable_agent_store_error(error: &storage::StorageError) -> String {
    let message = error.to_string();
    if message.contains("idx_agent_definitions_name_nocase")
        || message.contains("UNIQUE constraint failed")
    {
        "Agent name already exists in this project".to_string()
    } else {
        format!("Could not save project agent: {error}")
    }
}

pub(super) fn no_command_modifiers(keystroke: &Keystroke) -> bool {
    !keystroke.modifiers.platform && !keystroke.modifiers.control && !keystroke.modifiers.alt
}

pub(super) fn draft_matches_definition(
    draft: &AgentDefinitionDraft,
    definition: &AgentDefinition,
) -> bool {
    draft.id == Some(definition.id)
        && draft.name == definition.name
        && draft.runtime_profile == definition.runtime_profile
        && draft.workflow_description == definition.workflow_description
        && draft.enabled == definition.enabled
        && draft.sort_order == definition.sort_order
}

pub(super) fn next_default_agent_name(definitions: &[AgentDefinition]) -> String {
    let base = "New Agent";
    if !has_agent_name(definitions, base) {
        return base.to_string();
    }

    let mut index = 2;
    loop {
        let candidate = format!("{base} {index}");
        if !has_agent_name(definitions, &candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn has_agent_name(definitions: &[AgentDefinition], name: &str) -> bool {
    definitions
        .iter()
        .any(|definition| definition.name.eq_ignore_ascii_case(name))
}

impl AgentDefinitionDraft {
    pub(crate) fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
            enabled: true,
            sort_order: 0,
        }
    }
}
