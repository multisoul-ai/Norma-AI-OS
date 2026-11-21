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
        self.desktop_workspace.drawer_motion.retarget(true);
        self.wake_animation_frame_loop();
    }

    pub(super) fn cancel_workbench_leaving(&mut self, cx: &mut Context<Self>) {
        if let Some(PendingWorkbenchAction::CloseTabs {
            previous_selection,
            was_new_tab_page,
            ..
        }) = self.desktop_workspace.workbench.pending_close.take()
        {
            if let Some(id) = previous_selection {
                if !self
                    .desktop_workspace
                    .content_tabs
                    .iter()
                    .any(|tab| tab.selected && tab.id == id)
                {
                    self.select_desktop_content_tab(&id, cx);
                }
            }
            self.desktop_workspace.content_browser.new_tab_page = was_new_tab_page;
        }
        cx.notify();
    }

    pub(super) fn discard_workbench_before_leaving(&mut self, cx: &mut Context<Self>) {
        if let Some(PendingWorkbenchAction::CloseTabs { ids, .. }) =
            &self.desktop_workspace.workbench.pending_close
        {
            let ids = ids.clone();
            self.desktop_workspace.workbench.pending_close = None;
            self.commit_close_desktop_content_tabs(ids, cx);
        } else {
            self.desktop_workspace.workbench.drafts.clear();
            self.finish_workbench_leaving(cx);
        }
    }

    pub(crate) fn finish_workbench_leaving(&mut self, cx: &mut Context<Self>) {
        if self.desktop_workspace.workbench.has_edits() {
            return;
        }
        let Some(action) = self.desktop_workspace.workbench.pending_close.take() else {
            return;
        };
        if let PendingWorkbenchAction::CloseTabs { ids, .. } = action {
            self.commit_close_desktop_content_tabs(ids, cx);
            return;
        }
        cx.spawn(async move |this, cx| match action {
            PendingWorkbenchAction::Quit => {
                let _ = cx.update(crate::settings::general::quit_after_saving);
            }
            PendingWorkbenchAction::CloseWindow(window) => {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            }
            PendingWorkbenchAction::SwitchWorkspace(id, window) => {
                let _ = window.update(cx, |_, window, cx| {
                    this.update(cx, |app, cx| app.open_desktop_workspace(id, window, cx))
                });
            }
            PendingWorkbenchAction::CloseTabs { .. } => {}
        })
        .detach();
    }
}
