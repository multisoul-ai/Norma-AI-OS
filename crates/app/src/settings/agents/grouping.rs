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
        0 => ui::i18n::text("settings.no_agents_yet").to_string(),
        1 => ui::i18n::format(
            "settings.1_agent_on_canvas_on_canvas",
            &[("on_canvas", (on_canvas).to_string())],
        ),
        n => ui::i18n::format(
            "settings.n_agents_on_canvas_on_canvas",
            &[
                ("n", (n).to_string()),
                ("on_canvas", (on_canvas).to_string()),
            ],
        ),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/agent_settings_grouping_tests.rs"]
mod tests;
