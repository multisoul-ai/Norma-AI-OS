use super::MultisoulApp;
use gpui::{Context, Window};
use runtime::command::DockCommand;
use std::time::Duration;

impl MultisoulApp {
    pub(super) fn trace_command_dock(
        &self,
        stage: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = &self.command_dock else {
            tracing::info!(stage, entity = ?cx.entity_id(), screen = ?self.screen,
                settings_open = self.settings_panel.open, guide = ?self.feature_guide.presentation,
                "[help-diag] command input not initialized");
            return;
        };
        let suggestions = state.suggestions();
        let viewport = window.viewport_size();
        let composing = self.command_input_composing(window, cx);
        tracing::info!(
            stage, entity = ?cx.entity_id(), screen = ?self.screen,
            child_canvas = self.desktop_host.is_some(), nodes = self.runtime.document.nodes().len(),
            visible = state.visible, sidebar_origin = state.sidebar_origin,
            focused = self.command_input_focused(window, cx),
            composing,
            suggestions = suggestions.len(), selected = state.selected,
            help_selected = suggestions.get(state.selected)
                .is_some_and(|item| item.command == Some(DockCommand::Help)),
            settings_open = self.settings_panel.open, guide = ?self.feature_guide.presentation,
            window_active = window.is_window_active(),
            drawer_opacity = state.drawer.opacity(),
            panel_rect = ?self.command_panel_rect(viewport.width.into(), viewport.height.into()),
            "[help-diag] command dock state"
        );
    }

    pub(super) fn schedule_help_diagnostic(&self, window: &mut Window, cx: &mut Context<Self>) {
        if !self
            .command_dock
            .as_ref()
            .is_some_and(|state| state.draft.trim().eq_ignore_ascii_case("/help"))
        {
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            // 220ms drawer opening + 80ms margin; one snapshot, never per-frame logging.
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let _ = this.update_in(cx, |app, window, cx| {
                app.trace_command_dock("help_after_300ms", window, cx);
            });
        })
        .detach();
    }
}
