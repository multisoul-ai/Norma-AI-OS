use crate::app::MultisoulApp;
use crate::settings::agents::AgentDefinitionField;
use canvas::AgentProfile;
use gpui::{ClickEvent, Context, MouseDownEvent, Window};

impl MultisoulApp {
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

    pub(crate) fn on_agent_settings_toggle_draft_enabled(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.toggle_draft_enabled();
        self.agent_definitions.clear_status();
        cx.notify();
    }

    pub(crate) fn on_agent_settings_new_definition(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.create_new_card();
        cx.notify();
    }

    pub(crate) fn on_agent_settings_save(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                self.agent_definitions.set_status(
                    "Close the active Agent card before changing its profile or enabled state.",
                );
                cx.notify();
                return;
            }
        }
        if self.agent_definitions.save_draft() {
            let draft = self.agent_definitions.draft();
            if let Some(id) = draft.id {
                let _ = self.runtime.sync_agent_definition_name(id, &draft.name);
            }
        }
        cx.notify();
    }

    pub(crate) fn on_agent_settings_begin_edit(
        &mut self,
        field: AgentDefinitionField,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.begin_edit(field);
        self.focus_handle.focus(window);
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

    pub(crate) fn on_agent_settings_begin_card_edit(
        &mut self,
        id: i64,
        _: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.begin_card_edit(id);
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
        if !self
            .agent_definitions
            .prepare_create_card_from_definition(id)
        {
            cx.notify();
            return;
        }
        self.spawn_agent_card_from_definition(id, window, cx);
    }

    pub(crate) fn on_agent_settings_request_delete_definition(
        &mut self,
        id: i64,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.runtime.has_active_agent_definition(id) {
            self.agent_definitions
                .set_status("Close the active Agent card before deleting this Agent.");
            cx.notify();
            return;
        }
        self.agent_definitions.request_delete_definition(id);
        cx.notify();
    }
}
