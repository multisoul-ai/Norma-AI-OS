use crate::app::MultisoulApp;
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
        match self.agent_definitions.validate_draft() {
            Ok(()) => self
                .agent_definitions
                .set_status("Agent form is valid. Persistence wiring lands in Task 5."),
            Err(error) => self.agent_definitions.set_status(error),
        }
        cx.notify();
    }

    pub(crate) fn on_agent_settings_edit_definition(
        &mut self,
        id: i64,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.agent_definitions.load_draft_from_definition(id) {
            self.agent_definitions.clear_status();
        } else {
            self.agent_definitions
                .set_status("Could not load that agent into the draft.");
        }
        cx.notify();
    }

    pub(crate) fn on_agent_settings_delete_definition(
        &mut self,
        id: i64,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_definitions
            .set_status(format!("Delete for agent #{id} is UI-only in Task 3."));
        cx.notify();
    }
}
