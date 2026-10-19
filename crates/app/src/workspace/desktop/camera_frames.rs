use super::camera::DesktopCanvasSubject;
use crate::app::MultisoulApp;
use gpui::{Context, Window};

impl MultisoulApp {
    pub(super) fn schedule_desktop_zoom_frame(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_workspace.transition_frame_pending {
            return;
        }
        self.desktop_workspace.transition_frame_pending = true;
        cx.on_next_frame(window, |app, window, cx| {
            app.desktop_workspace.transition_frame_pending = false;
            let now = cx.background_executor().now();
            let delta = now.saturating_duration_since(app.desktop_workspace.transition_last_tick);
            app.desktop_workspace.transition_last_tick = now;
            if app.desktop_project_reflow_active() {
                app.desktop_workspace.project_layout.tick(delta);
                if app.desktop_project_reflow_active() {
                    app.schedule_desktop_zoom_frame(window, cx);
                }
                cx.notify();
            }
            if let Some(transition) = app.desktop_workspace.transition.as_mut() {
                if let Some(log) = app.desktop_workspace.motion_log.as_mut() {
                    log.record_callback(now, delta);
                }
                transition.tick(delta);
                if transition.is_active() {
                    app.schedule_desktop_zoom_frame(window, cx);
                } else {
                    app.finish_desktop_zoom(window, cx);
                }
                cx.notify();
            }
        });
    }

    pub(super) fn finish_desktop_zoom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // While a transition is still present, sync snaps project slots to the
        // destination canvas width. This keeps the first settled frame aligned.
        if matches!(self.screen, crate::app::AppScreen::DesktopWorkspace) {
            self.sync_desktop_project_layout(window, cx);
            self.desktop_workspace.project_layout.settle();
        }
        let Some(finished) = self.desktop_workspace.transition.take() else {
            return;
        };
        self.desktop_workspace.finish_motion_log(
            if finished.is_active() {
                "forced_finish"
            } else {
                "completed"
            },
            cx.background_executor().now(),
        );
        match finished.subject {
            DesktopCanvasSubject::Workspace(workspace_id) => {
                if self.desktop_workspace.pending_navigation.is_none()
                    && matches!(self.screen, crate::app::AppScreen::DesktopWorkspace)
                    && self.desktop_workspace.active_id.as_deref() == Some(&workspace_id)
                {
                    if let Some(area_id) = self.desktop_workspace.expanded_area_id.clone() {
                        self.expand_desktop_project_area(area_id, window, cx);
                    }
                }
            }
            DesktopCanvasSubject::Project {
                workspace_id,
                area_id,
            } => {
                if let Some(session) = self
                    .desktop_workspace
                    .sessions
                    .get(&(workspace_id, area_id.clone()))
                {
                    if self.desktop_workspace.expanded_area_id.as_deref() != Some(&area_id) {
                        session.update(cx, |session, cx| session.detach_project_window(cx));
                    } else {
                        session.update(cx, |_, cx| cx.notify());
                    }
                }
            }
        }
        self.finish_desktop_navigation(window, cx);
        // The directory does not render the workspace again. Resume a paused reflow
        // here as well, so its live thumbnail cannot remain at an intermediate layout.
        if self.desktop_project_reflow_active() {
            self.desktop_workspace.transition_last_tick = cx.background_executor().now();
            self.schedule_desktop_zoom_frame(window, cx);
        }
    }
}
