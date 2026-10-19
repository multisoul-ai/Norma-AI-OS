use super::*;
use crate::app::MultisoulApp;
use gpui::Context;

impl MultisoulApp {
    pub(crate) fn allow_workbench_workspace_switch(
        &mut self,
        id: &str,
        window: &gpui::Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.desktop_workspace.active_id.as_deref() == Some(id)
            || self.confirm_preview_edits_before_leaving(cx)
        {
            return true;
        }
        if self.desktop_workspace.workbench.has_edits() {
            self.desktop_workspace.workbench.pending_close = Some(
                PendingWorkbenchAction::SwitchWorkspace(id.to_string(), window.window_handle()),
            );
        }
        false
    }

    pub(crate) fn reveal_workbench_prompt(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self
            .desktop_workspace
            .content_tabs
            .iter()
            .find(|t| t.kind == storage::DesktopContentKind::ProjectFiles)
            .map(|t| t.id.clone())
        {
            self.select_desktop_content_tab(&id, cx);
        }
        self.desktop_workspace.drawers.right_open = true;
        self.desktop_workspace
            .drawer_motion
            .retarget(self.desktop_workspace.drawers.left_open, true);
        self.wake_animation_frame_loop();
    }

    pub(crate) fn finish_workbench_leaving(&mut self, cx: &mut Context<Self>) {
        if self.desktop_workspace.workbench.has_edits() {
            return;
        }
        let Some(action) = self.desktop_workspace.workbench.pending_close.take() else {
            return;
        };
        if let PendingWorkbenchAction::CloseTab(id) = action {
            self.close_desktop_content_tab(&id, cx);
            return;
        }
        cx.spawn(async move |this, cx| match action {
            PendingWorkbenchAction::Quit => {
                let _ = cx.update(|cx| crate::settings::general::quit_after_saving(cx));
            }
            PendingWorkbenchAction::CloseWindow(window) => {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            }
            PendingWorkbenchAction::SwitchWorkspace(id, window) => {
                let _ = window.update(cx, |_, window, cx| {
                    this.update(cx, |app, cx| app.open_desktop_workspace(id, window, cx))
                });
            }
            PendingWorkbenchAction::CloseTab(_) => {}
        })
        .detach();
    }
}
