use crate::app::MultisoulApp;
use crate::settings::agents::AgentDefinitionField;
use canvas::AgentProfile;
use gpui::{ClickEvent, Context, Window};

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

    pub(crate) fn on_agent_settings_reset_draft(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.reset_draft();
        self.agent_definitions.clear_status();
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

    pub(crate) fn on_agent_settings_edit_definition(
        &mut self,
        id: i64,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.edit_definition(id);
        cx.notify();
    }

    pub(crate) fn on_agent_settings_delete_definition(
        &mut self,
        id: i64,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.delete_definition(id);
        cx.notify();
    }

    pub(crate) fn on_agent_settings_toggle_definition(
        &mut self,
        id: i64,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions.toggle_enabled(id);
        cx.notify();
    }
}
