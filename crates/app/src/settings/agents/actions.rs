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
        self.agent_definitions.save_draft();
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
        self.agent_definitions.request_delete_definition(id);
        cx.notify();
    }
}
