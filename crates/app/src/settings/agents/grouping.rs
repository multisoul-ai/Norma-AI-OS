use std::collections::HashSet;
use storage::AgentDefinition;

pub(super) struct AgentGroups<'a> {
    pub(super) on_canvas: Vec<&'a AgentDefinition>,
    pub(super) library: Vec<&'a AgentDefinition>,
}

pub(super) fn group_definitions<'a>(
    definitions: Vec<&'a AgentDefinition>,
    on_canvas_ids: &HashSet<i64>,
) -> AgentGroups<'a> {
    let (on_canvas, library) = definitions
        .into_iter()
        .partition(|definition| on_canvas_ids.contains(&definition.id));
    AgentGroups { on_canvas, library }
}

pub(super) fn roster_summary(total: usize, on_canvas: usize) -> String {
    match total {
        0 => "No agents yet".to_string(),
        1 => format!("1 agent · {on_canvas} on Canvas"),
        n => format!("{n} agents · {on_canvas} on Canvas"),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/agent_settings_grouping_tests.rs"]
mod tests;
