//! Persist the left-drawer view with the desktop workspace. Writes happen on a
//! background thread when the view changes, when the workspace changes, or when
//! the desktop host exits. Render and pointer paths do not touch SQLite.

use crate::app::MultisoulApp;
use gpui::{AppContext, Context};
use runtime::desktop_drawers::{
    left_view_from_preference, left_view_preference_json, restore_left_view,
};
use std::sync::atomic::Ordering;

impl MultisoulApp {
    pub(crate) fn persist_left_drawer_view(&mut self, workspace_id: &str, cx: &mut Context<Self>) {
        let json = left_view_preference_json(self.desktop_workspace.drawers.left_view);
        let Some(workspace) = self
            .desktop_workspace
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id == workspace_id)
        else {
            return;
        };
        if workspace.drawer_layout_json.as_deref() == Some(json) {
            return;
        }
        workspace.drawer_layout_json = Some(json.to_string());
        let store = self.desktop_workspace.store.clone();
        let id = workspace_id.to_string();
        let saved = json.to_string();
        let ticket = self
            .desktop_workspace
            .drawer_layout_generation
            .fetch_add(1, Ordering::SeqCst)
            + 1;
        let generation = std::sync::Arc::clone(&self.desktop_workspace.drawer_layout_generation);
        let previous = self.desktop_workspace.drawer_layout_save.take();
        self.desktop_workspace.drawer_layout_save = Some(cx.spawn(async move |_, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            // A newer toggle or the exit flush owns the row once the ticket moves on.
            if generation.load(Ordering::SeqCst) != ticket {
                return;
            }
            let _ = cx
                .background_spawn(async move { store.set_drawer_layout(&id, &saved) })
                .await;
        }));
    }

    pub(crate) fn activate_desktop_workspace_view(&mut self, id: &str, cx: &mut Context<Self>) {
        let previous = self.desktop_workspace.active_id.clone();
        let switching = previous.as_deref() != Some(id);
        if switching {
            if let Some(previous) = previous.as_deref() {
                self.persist_left_drawer_view(previous, cx);
            }
        }
        self.desktop_workspace.active_id = Some(id.to_string());
        if switching {
            self.restore_left_drawer_view(id);
        }
    }

    pub(super) fn restore_left_drawer_view(&mut self, workspace_id: &str) {
        let raw = self
            .desktop_workspace
            .workspaces
            .iter()
            .find(|workspace| workspace.id == workspace_id)
            .and_then(|workspace| workspace.drawer_layout_json.clone());
        let view = left_view_from_preference(raw.as_deref());
        let before = self.desktop_workspace.drawers;
        self.desktop_workspace.drawers = restore_left_view(before, view);
        self.desktop_workspace.left_view_slide = super::left_view_slide::LeftViewSlide::settled();
        if before != self.desktop_workspace.drawers {
            self.desktop_workspace.drawer_motion.retarget(
                self.desktop_workspace.drawers.left_open,
                self.desktop_workspace.drawers.right_open,
            );
        }
    }

    pub(crate) fn flush_left_drawer_view_on_exit(&mut self) {
        let Some(id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let json = left_view_preference_json(self.desktop_workspace.drawers.left_view).to_string();
        if let Some(workspace) = self
            .desktop_workspace
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id == id)
        {
            workspace.drawer_layout_json = Some(json.clone());
        }
        let _ = self
            .desktop_workspace
            .drawer_layout_generation
            .fetch_add(1, Ordering::SeqCst);
        let store = self.desktop_workspace.store.clone();
        let _ = std::thread::spawn(move || store.set_drawer_layout(&id, &json)).join();
    }
}
