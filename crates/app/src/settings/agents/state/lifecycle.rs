use super::{support, AgentDefinition, AgentDefinitionsState, AgentProfile};

impl AgentDefinitionsState {
    pub(crate) fn creation_store(&self) -> Option<storage::AgentDefinitionStore> {
        self.store.clone()
    }

    /// Merge only this creation, so a delayed background result cannot erase another creator.
    pub(crate) fn install_created_definition(&mut self, definition: AgentDefinition) {
        self.definitions
            .retain(|existing| existing.id != definition.id);
        self.definitions.push(definition);
        self.status = None;
    }

    pub(crate) fn forget_created_definition(&mut self, id: i64) {
        self.definitions.retain(|definition| definition.id != id);
    }
    pub(crate) fn create_definition(
        &mut self,
        requested_name: Option<String>,
        runtime_profile: AgentProfile,
    ) -> Result<AgentDefinition, String> {
        self.create_definition_with_origin(
            requested_name,
            runtime_profile,
            storage::AgentDefinitionOrigin::Explicit,
        )
    }

    pub(crate) fn create_quick_definition(
        &mut self,
        runtime_profile: AgentProfile,
    ) -> Result<AgentDefinition, String> {
        self.create_definition_with_origin(
            None,
            runtime_profile,
            storage::AgentDefinitionOrigin::QuickCard,
        )
    }

    fn create_definition_with_origin(
        &mut self,
        requested_name: Option<String>,
        runtime_profile: AgentProfile,
        origin: storage::AgentDefinitionOrigin,
    ) -> Result<AgentDefinition, String> {
        let store = self.store.as_ref().ok_or_else(|| {
            ui::i18n::text("settings.open_a_project_before_creating_agents").to_string()
        })?;
        let name =
            requested_name.unwrap_or_else(|| support::next_default_agent_name(&self.definitions));
        if name.is_empty() {
            return Err(ui::i18n::text(
                "settings.could_not_allocate_a_unique_agent_name_specify_a_custom_name",
            )
            .into());
        }
        agent::AgentName::parse(&name).map_err(|error| error.to_string())?;
        let input = storage::NewAgentDefinition {
            name,
            runtime_profile,
            workflow_description: String::new(),
        };
        let created = match origin {
            storage::AgentDefinitionOrigin::Explicit => store.create(&input),
            storage::AgentDefinitionOrigin::QuickCard => store.create_quick(&input),
        }
        .map_err(|error| support::readable_agent_store_error(&error).to_string())?;
        self.reload();
        self.select_after_reload(Some(created.id));
        Ok(created)
    }

    pub(crate) fn cleanup_quick_definition(&mut self, id: i64) -> Result<bool, String> {
        let store = self.store.as_ref().ok_or_else(|| {
            ui::i18n::text("settings.open_a_project_before_cleaning_up_agents").to_string()
        })?;
        let deleted = store.delete_if_quick(id).map_err(|error| {
            ui::i18n::format(
                "settings.could_not_clean_up_temporary_agent_error",
                &[("error", (error).to_string())],
            )
        })?;
        if deleted {
            let fallback_id = self.next_selection_after_delete(id);
            self.reload();
            self.select_after_reload(fallback_id);
        }
        Ok(deleted)
    }
}
