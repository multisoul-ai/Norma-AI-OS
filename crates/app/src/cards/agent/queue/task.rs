use super::*;

pub(super) fn task_summary(task: &str, theme: Theme) -> Div {
    div()
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        .truncate()
        .text_size(px(12.0))
        .text_color(rgba(theme.text_primary))
        .child(task.to_string())
}

pub(super) fn task_detail(task: &AgentQueueTask, theme: Theme) -> Div {
    div()
        .mx(px(10.0))
        .mb(px(10.0))
        .p(px(10.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .rounded(px(8.0))
        .bg(rgba(with_alpha(theme.panel_high, 0x78)))
        .text_size(px(11.0))
        .text_color(rgba(theme.text_secondary))
        .child(task.task.clone())
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgba(theme.text_tertiary))
                .child(ui::i18n::format(
                    "cards.task_reference",
                    &[("arg0", (task.task_ref).to_string())],
                )),
        )
}

pub(super) fn source_pill(
    app: &MultisoulApp,
    task_id: u64,
    waiting_index: Option<usize>,
    source: AgentQueueTaskSource,
    theme: Theme,
) -> impl IntoElement {
    let (label, slug) = match source {
        AgentQueueTaskSource::Norma => ("Norma".to_string(), "norma".to_string()),
        AgentQueueTaskSource::Manual => (
            ui::i18n::text("cards.manual").to_string(),
            "manual".to_string(),
        ),
        AgentQueueTaskSource::Agent { definition_id } => {
            let name = app
                .agent_definitions
                .definitions()
                .iter()
                .find(|definition| definition.id == definition_id)
                .map(|definition| definition.name.clone())
                .unwrap_or_else(|| ui::i18n::text("cards.agent").to_string());
            (name, "agent".to_string())
        }
    };
    let selector = waiting_index.map(|index| format!("agent-queue-source-{index}-{slug}"));
    div()
        .id(("agent-queue-source", task_id))
        .when_some(selector, |pill, selector| {
            pill.debug_selector(move || selector.clone())
        })
        .flex_none()
        .px(px(7.0))
        .h(px(22.0))
        .flex()
        .items_center()
        .rounded(px(11.0))
        .bg(rgba(with_alpha(theme.panel_high, 0x88)))
        .text_size(px(9.0))
        .text_color(rgba(theme.text_tertiary))
        .child(label)
}
