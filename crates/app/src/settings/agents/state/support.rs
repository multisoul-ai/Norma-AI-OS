use super::{AgentDefinitionDraft, AgentDefinitionField, MAX_WORKFLOW_DESCRIPTION_CHARS};
use canvas::AgentProfile;
#[cfg(test)]
use gpui::Keystroke;
use storage::AgentDefinition;

pub(crate) fn validate_agent_definition_text(
    name: &str,
    workflow_description: &str,
) -> Result<(), ui::i18n::LocalizedText> {
    let name = name.trim();
    let workflow = workflow_description.trim();

    agent::AgentName::parse(name).map_err(|_| {
        ui::i18n::LocalizedText::new("settings.agent_name_must_be_2_16_english_letters_or_2_8")
    })?;
    if workflow.chars().count() > MAX_WORKFLOW_DESCRIPTION_CHARS {
        return Err(ui::i18n::LocalizedText::new(
            "settings.workflow_description_must_be_2000_characters_or_fewer",
        ));
    }

    Ok(())
}

pub(super) fn display_value(field: AgentDefinitionField, value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        match field {
            AgentDefinitionField::Name => ui::i18n::text("settings.set_the_agent_name").to_string(),
            AgentDefinitionField::WorkflowDescription => {
                ui::i18n::text("settings.describe_the_workflow_this_agent_should_follow")
                    .to_string()
            }
        }
    } else {
        value.to_string()
    }
}

pub(super) fn readable_agent_store_error(error: &storage::StorageError) -> ui::i18n::LocalizedText {
    if matches!(error, storage::StorageError::AgentNameConflict { .. }) {
        return ui::i18n::LocalizedText::new("settings.agent_name_already_exists_in_this_project");
    }
    let message = error.to_string();
    if message.contains("idx_agent_definitions_name_nocase")
        || message.contains("UNIQUE constraint failed")
    {
        ui::i18n::LocalizedText::new("settings.agent_name_already_exists_in_this_project")
    } else {
        ui::i18n::message(
            "settings.could_not_save_project_agent_error",
            &[("error", (error).to_string())],
        )
    }
}

#[cfg(test)]
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
