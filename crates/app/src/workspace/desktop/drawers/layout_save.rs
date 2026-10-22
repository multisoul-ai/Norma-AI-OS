//! Persist the left-drawer view with the desktop workspace. Writes happen on a
//! background thread when the view changes, when the workspace changes, or when
//! the desktop host exits. Render and pointer paths do not touch SQLite.

use super::super::overview_layout::OverviewCollapse;
use crate::app::MultisoulApp;
use gpui::{AppContext, Context};
use runtime::desktop_drawers::{
    overview_layout_from_preference, overview_layout_json, restore_overview_layout,
};
use std::sync::atomic::Ordering;

impl MultisoulApp {
    pub(in crate::workspace::desktop) fn persist_left_drawer_view(
        &mut self,
        workspace_id: &str,
        cx: &mut Context<Self>,
    ) {
        let json = overview_layout_json(&self.desktop_workspace.drawers);
        let Some(workspace) = self
            .desktop_workspace
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id == workspace_id)
        else {
            return;
        };
        if workspace.drawer_layout_json.as_deref() == Some(json.as_str()) {
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

    /// 运行概览各栏的折叠状态（存在抽屉状态里，随布局一起保存）。
    pub(in crate::workspace::desktop) fn overview_collapse(&self) -> OverviewCollapse {
        OverviewCollapse::from_bits(self.desktop_workspace.drawers.overview_collapsed)
    }

    /// 修改折叠状态：立即重绘，并在后台把布局写入当前工作空间。
    pub(in crate::workspace::desktop) fn update_overview_collapse(
        &mut self,
        change: impl FnOnce(&mut OverviewCollapse),
        cx: &mut Context<Self>,
    ) {
        let mut collapse = self.overview_collapse();
        change(&mut collapse);
        if collapse.bits() == self.desktop_workspace.drawers.overview_collapsed {
            return;
        }
        self.desktop_workspace.drawers.overview_collapsed = collapse.bits();
        if let Some(id) = self.desktop_workspace.active_id.clone() {
            self.persist_left_drawer_view(&id, cx);
        }
        cx.notify();
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
            // 切换工作空间时清掉旧缓存并后台重读，标签角标才不会显示上一个空间的数量。
            self.desktop_workspace.ideas.clear();
            self.desktop_workspace.ideas_menu = None;
            self.desktop_workspace.ideas_aux.picker = None;
            self.desktop_workspace.ideas_cursor = None;
            self.refresh_ideas(cx);
        }
    }

    pub(super) fn restore_left_drawer_view(&mut self, workspace_id: &str) {
        let raw = self
            .desktop_workspace
            .workspaces
            .iter()
            .find(|workspace| workspace.id == workspace_id)
            .and_then(|workspace| workspace.drawer_layout_json.clone());
        let preference = overview_layout_from_preference(raw.as_deref());
        let before = self.desktop_workspace.drawers;
        self.desktop_workspace.drawers = restore_overview_layout(before, preference, None);
        if preference.floating {
            self.desktop_workspace.overview_float_fade.snap_visible();
        } else {
            self.desktop_workspace.overview_float_fade = super::float::OverviewFloatFade::hidden();
        }
        self.desktop_workspace.left_view_slide = super::left_view_slide::LeftViewSlide::settled();
        if before != self.desktop_workspace.drawers {
            self.desktop_workspace.drawer_motion.settle(
                self.desktop_workspace.drawers.left_open,
                self.desktop_workspace.drawers.right_open,
            );
        }
    }

    pub(crate) fn flush_left_drawer_view_on_exit(&mut self) {
        let Some(id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let json = overview_layout_json(&self.desktop_workspace.drawers);
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

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_overview_collapse_tests.rs"]
mod collapse_tests;
