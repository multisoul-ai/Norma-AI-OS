use super::{AgentDefinition, AgentDefinitionsState};

impl AgentDefinitionsState {
    pub(crate) fn visible_definitions(&self) -> Vec<&AgentDefinition> {
        let query = self.search_query.trim().to_lowercase();
        if query.is_empty() {
            return self.definitions.iter().collect();
        }
        self.definitions
            .iter()
            .filter(|definition| {
                definition.name.to_lowercase().contains(&query)
                    || definition
                        .runtime_profile
                        .display_title()
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
}
