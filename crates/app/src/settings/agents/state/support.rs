use super::{AgentDefinitionDraft, AgentDefinitionField, MAX_WORKFLOW_DESCRIPTION_CHARS};
use canvas::AgentProfile;
use gpui::Keystroke;
use storage::AgentDefinition;

pub(crate) fn validate_agent_definition_text(
    name: &str,
    workflow_description: &str,
) -> Result<(), String> {
    let name = name.trim();
    let workflow = workflow_description.trim();

    agent::AgentName::parse(name).map_err(|_| {
        "Agent name must be 2–16 English letters or 2–8 Chinese characters".to_string()
    })?;
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
    if matches!(error, storage::StorageError::AgentNameConflict { .. }) {
        return "Agent name already exists in this project".to_string();
    }
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
    agent::AgentNameRegistry::from_names(
        definitions
            .iter()
            .map(|definition| definition.name.as_str()),
    )
    .ok()
    .and_then(|registry| registry.next_available_name())
    .unwrap_or_default()
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
