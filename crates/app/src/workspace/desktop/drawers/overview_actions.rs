//! Run-overview actions. Rendering reads the cached projection; these handlers
//! do not recompute groups or touch SQLite on the pointer path.

use super::super::overview_layout::OverviewGroup;
use super::super::overview_rows::{
    move_overview_cursor, overview_action_keeps_overview, present_overview, OverviewAction,
    OverviewCursor, OverviewKey, OverviewScreen,
};
use crate::app::MultisoulApp;
use gpui::{Context, Window};
use std::time::{SystemTime, UNIX_EPOCH};

impl MultisoulApp {
    pub(super) fn presented_overview(&self) -> OverviewScreen {
        let now = unix_ms();
        let stops = self.overview_ego_stops();
        let mut screen = match &self.desktop_workspace.activity_run_overview {
            Some(overview) => present_overview(overview, &stops, now),
            None => OverviewScreen::empty(),
        };
        // 折叠的栏不显示行，键盘上下也不应落到看不见的行上。
        let groups = [
            OverviewGroup::Attention,
            OverviewGroup::Running,
            OverviewGroup::Idle,
        ];
        for (section, group) in screen.sections.iter_mut().zip(groups) {
            if self.overview_collapse().is_collapsed(group) {
                section.rows.clear();
            }
        }
        screen
    }

    /// 统计卡点击：展开目标分组，并把键盘光标放到它的第一行（没有行就只展开）。
    pub(super) fn jump_to_overview_group(&mut self, group: OverviewGroup, cx: &mut Context<Self>) {
        self.update_overview_collapse(|collapse| collapse.reveal(group), cx);
        let index = match group {
            OverviewGroup::Attention => 0,
            OverviewGroup::Running => 1,
            OverviewGroup::Idle => 2,
            OverviewGroup::Ideas => return,
        };
        let screen = self.presented_overview();
        if screen
            .sections
            .get(index)
            .is_some_and(|item| item.rows.is_empty())
        {
            return;
        }
        let first: usize = screen.sections[..index]
            .iter()
            .map(|item| item.rows.len())
            .sum();
        self.desktop_workspace.overview_cursor = Some(first);
        let target = screen.row(first).and_then(|row| row.prelight.clone());
        self.prelight_overview(target, cx);
        cx.notify();
    }

    pub(super) fn handle_run_overview_key(
        &mut self,
        key: &str,
        modified: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if modified || self.ideas_hold_focus(window, cx) {
            return false;
        }
        let nav = match key {
            "up" => OverviewKey::Up,
            "down" => OverviewKey::Down,
            "home" => OverviewKey::Home,
            "end" => OverviewKey::End,
            "enter" => OverviewKey::Enter,
            _ => return false,
        };
        let screen = self.presented_overview();
        let Some(effect) = move_overview_cursor(
            self.desktop_workspace.overview_cursor,
            screen.row_count(),
            nav,
        ) else {
            return true;
        };
        match effect {
            OverviewCursor::Move(index) => {
                self.desktop_workspace.overview_cursor = Some(index);
                let target = screen.row(index).and_then(|row| row.prelight.clone());
                self.prelight_overview(target, cx);
                cx.notify();
            }
            OverviewCursor::Activate(index) => {
                if let Some(row) = screen.row(index).filter(|row| row.enabled) {
                    let action = row.action.clone();
                    self.activate_overview_action(action, window, cx);
                }
            }
        }
        true
    }

    pub(super) fn activate_overview_action(
        &mut self,
        action: OverviewAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if overview_action_keeps_overview(&action) {
            self.perform_overview_action(action, window, cx);
            let focus = self.left_overview_focus(cx);
            window.focus(&focus);
            return;
        }
        if let OverviewAction::SwitchProject { area_id } = action {
            self.expand_desktop_project_area(area_id, window, cx);
        }
    }

    pub(super) fn prelight_overview(
        &mut self,
        target: Option<(String, u64)>,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_workspace.overview_prelight == target {
            return;
        }
        if let Some((area, node)) = self.desktop_workspace.overview_prelight.clone() {
            self.set_overview_card_hover(&area, node, false, cx);
        }
        if let Some((area, node)) = target.clone() {
            self.set_overview_card_hover(&area, node, true, cx);
        }
        self.desktop_workspace.overview_prelight = target;
    }

    fn perform_overview_action(
        &mut self,
        action: OverviewAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            OverviewAction::LocateApproval {
                area_id,
                node_id,
                run_id,
                turn_id,
            } => {
                self.locate_desktop_approval(
                    &area_id,
                    node_id,
                    Some(run_id.as_str()),
                    Some(turn_id.as_str()),
                    window,
                    cx,
                );
            }
            OverviewAction::LocateAgent { area_id, node_id } => {
                self.locate_desktop_agent(&area_id, node_id, window, cx);
            }
            OverviewAction::StopEgo {
                area_id,
                node_id,
                agent_id,
            } => {
                self.stop_desktop_ego_task(&area_id, node_id, agent_id, cx);
            }
            OverviewAction::OpenReview {
                area_id,
                paths,
                direct_diff,
            } => {
                self.open_turn_changes(area_id, paths, direct_diff, cx);
            }
            OverviewAction::OpenFile {
                area_id,
                node_id,
                path,
                fingerprint,
            } => {
                self.open_overview_file(&area_id, node_id, &path, &fingerprint, cx);
            }
            OverviewAction::SwitchProject { .. } => {}
        }
    }

    fn open_turn_changes(
        &mut self,
        area_id: String,
        paths: Vec<String>,
        direct_diff: bool,
        cx: &mut Context<Self>,
    ) {
        self.open_file_workbench(cx);
        let first = paths.first().cloned();
        let state = &mut self.desktop_workspace.workbench;
        state.lock_area = true;
        state.saved.area_id = Some(area_id);
        state.changes_section_closed = false;
        state.turn_paths = Some(paths);
        state.pending_turn_diff = if direct_diff { first } else { None };
        state.project_area = None;
    }

    fn open_overview_file(
        &mut self,
        area_id: &str,
        node_id: u64,
        path: &str,
        fingerprint: &str,
        cx: &mut Context<Self>,
    ) {
        let artifact = self
            .desktop_workspace
            .content_artifacts
            .iter()
            .find(|item| {
                item.area_id == area_id
                    && item.node_id == node_id
                    && item.source_path == path
                    && item.version == fingerprint
            })
            .cloned();
        if let Some(artifact) = artifact {
            self.open_desktop_artifact(artifact, cx);
            return;
        }
        self.desktop_workspace.error = Some("文件已不在待查看列表".into());
        cx.notify();
    }

    fn set_overview_card_hover(
        &mut self,
        area_id: &str,
        node_id: u64,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id, area_id.to_owned()))
            .cloned()
        else {
            return;
        };
        session.update(cx, |app, cx| {
            app.motion.set_hover(node_id, hovered);
            cx.notify();
        });
    }

    fn overview_ego_stops(&self) -> Vec<(String, u64, u64)> {
        self.desktop_workspace
            .areas
            .iter()
            .flat_map(|area| {
                self.overview_rows_for(&area.id)
                    .into_iter()
                    .filter_map(|row| {
                        row.ego_stop_agent
                            .map(|agent| (row.area_id, row.node_id, agent))
                    })
            })
            .collect()
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}
