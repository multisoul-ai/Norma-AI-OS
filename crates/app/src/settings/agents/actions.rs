use crate::app::MultisoulApp;
use crate::settings::agents::{AgentDefinitionField, AgentStatusFilter};
use canvas::AgentProfile;
use gpui::{ClickEvent, Context, MouseDownEvent, Window};

impl MultisoulApp {
    pub(crate) fn on_agent_settings_set_status_filter(
        &mut self,
        filter: AgentStatusFilter,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.agent_definitions.set_status_filter(filter) {
            cx.notify();
        }
    }

    pub(crate) fn on_agent_settings_select_profile(
        &mut self,
        profile: AgentProfile,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.set_draft_runtime_profile(profile);
        self.agent_definitions.clear_status();
        cx.notify();
    }

    pub(crate) fn on_agent_settings_new_definition(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.agent_definitions.create_new_card() {
            self.agent_definitions
                .begin_edit(AgentDefinitionField::Name);
        }
        cx.notify();
    }

    pub(crate) fn on_agent_settings_save(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_agent_settings_draft();
        cx.notify();
    }

    fn save_agent_settings_draft(&mut self) -> bool {
        let draft = self.agent_definitions.draft().clone();
        if let Some(id) = draft.id {
            let active = self.runtime.has_active_agent_definition(id);
            let persisted = self
                .agent_definitions
                .definitions()
                .iter()
                .find(|definition| definition.id == id);
            let changes_runtime = persisted.is_some_and(|definition| {
                definition.runtime_profile != draft.runtime_profile
                    || definition.enabled != draft.enabled
            });
            if active && changes_runtime {
                self.agent_definitions
                    .set_status(ui::i18n::LocalizedText::new(
                    "settings.close_the_active_agent_card_before_changing_its_profile_or_enabled",
                ));
                return false;
            }
        }
        let saved = self.agent_definitions.save_draft();
        // Configuration labels never rename card roles. Roles have their own exact-instance editor.
        saved
    }

    pub(crate) fn on_agent_settings_begin_edit(
        &mut self,
        field: AgentDefinitionField,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.begin_edit(field);
        cx.notify();
    }

    pub(crate) fn on_agent_settings_select_definition(
        &mut self,
        id: i64,
        _: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.select_definition(id);
        cx.notify();
    }

    pub(crate) fn on_agent_settings_cancel_card_edit(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.cancel_card_edit();
        cx.notify();
    }

    pub(crate) fn on_agent_settings_create_card(
        &mut self,
        id: i64,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.agent_definitions.is_dirty() && !self.save_agent_settings_draft() {
            cx.notify();
            return;
        }
        if !self
            .agent_definitions
            .prepare_create_card_from_definition(id)
        {
            cx.notify();
            return;
        }
        self.spawn_agent_card_from_definition(id, window, cx);
    }

    pub(crate) fn on_agent_settings_show_on_canvas(
        &mut self,
        id: i64,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let node_id = self.runtime.document.nodes().iter().find_map(|node| {
            let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            (spec.definition_id == Some(id)).then_some(node.id)
        });
        let Some(node_id) = node_id else {
            self.agent_definitions
                .set_status(ui::i18n::LocalizedText::new(
                    "settings.that_agent_is_no_longer_on_canvas",
                ));
            cx.notify();
            return;
        };
        crate::canvas::spawn_focus::activate_spawned_node(&mut self.runtime, node_id);
        self.close_settings_panel(cx);
    }

    pub(crate) fn on_agent_settings_request_delete_definition(
        &mut self,
        id: i64,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let blocked = self.runtime.has_active_agent_definition(id);
        self.agent_definitions.open_delete_dialog(id, blocked);
        cx.notify();
    }

    pub(crate) fn on_agent_settings_cancel_delete(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.close_delete_dialog();
        cx.notify();
    }

    pub(crate) fn on_agent_settings_confirm_delete(
        &mut self,
        id: i64,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.runtime.has_active_agent_definition(id) {
            self.agent_definitions.open_delete_dialog(id, true);
        } else {
            self.agent_definitions.confirm_delete_from_dialog(id);
        }
        cx.notify();
    }
}
