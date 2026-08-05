use super::{AgentDefinition, AgentDefinitionsState};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum AgentStatusFilter {
    #[default]
    All,
    Enabled,
    Disabled,
}

impl AgentDefinitionsState {
    pub(crate) fn visible_definitions(&self) -> Vec<&AgentDefinition> {
        let query = self.search_query.trim().to_lowercase();
        self.definitions
            .iter()
            .filter(|definition| {
                matches!(
                    (self.status_filter, definition.enabled),
                    (AgentStatusFilter::All, _)
                        | (AgentStatusFilter::Enabled, true)
                        | (AgentStatusFilter::Disabled, false)
                )
            })
            .filter(|definition| {
                query.is_empty()
                    || definition.name.to_lowercase().contains(&query)
                    || definition
                        .runtime_profile
                        .display_title()
                        .to_lowercase()
                        .contains(&query)
                    || definition
                        .workflow_description
                        .to_lowercase()
                        .contains(&query)
            })
            .collect()
    }

    pub(crate) fn search_query(&self) -> &str {
        &self.search_query
    }

    pub(crate) fn set_search_query(&mut self, query: String) -> bool {
        if self.search_query == query {
            return false;
        }
        self.search_query = query;
        true
    }

    pub(crate) fn status_filter(&self) -> AgentStatusFilter {
        self.status_filter
    }

    pub(crate) fn set_status_filter(&mut self, filter: AgentStatusFilter) -> bool {
        if self.status_filter == filter {
            return false;
        }
        self.status_filter = filter;
        true
    }
}
