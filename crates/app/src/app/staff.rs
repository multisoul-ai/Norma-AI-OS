//! Staff UI state is a cache; TaskStore is the authoritative writer.
use super::MultisoulApp;
use gpui::{Context, Entity};
use norma::protocol::Task;
use std::collections::BTreeMap;
use ui::component::input::InputState;
mod actions;
mod composer;
mod detail;
mod messages;
mod task_list;
mod view;

/// 与原型相同：窄桌面压缩列表，保留对话宽度。
pub(crate) fn sidebar_width(window: &gpui::Window) -> f32 {
    let width = f32::from(window.viewport_size().width);
    if width < 760.0 {
        0.0
    } else if width < 1180.0 {
        290.0
    } else {
        330.0
    }
}

#[derive(Default)]
pub(crate) struct StaffState {
    pub enabled: bool,
    pub fullscreen: bool,
    pub loaded: bool,
    pub loading: bool,
    pub busy: bool,
    pub tasks: BTreeMap<String, Task>,
    pub selected: Option<String>,
    pub drafts: BTreeMap<String, String>,
    pub input: Option<Entity<InputState>>,
    pub search: Option<Entity<InputState>>,
    pub query: String,
    pub filter: Option<norma::protocol::TaskDisplayStatus>,
    pub collapsed: std::collections::BTreeSet<String>,
    pub scroll: gpui::ScrollHandle,
    pub error: Option<ui::i18n::LocalizedText>,
    pub pending: BTreeMap<u64, PendingWorker>,
    pub events: std::cell::RefCell<std::collections::VecDeque<brain::BrainEvent>>,
    pub saving_event: bool,
    pub recorded: std::cell::RefCell<
        std::collections::VecDeque<(brain::BrainEvent, brain::staff::TaskEventResult)>,
    >,
}

pub(crate) struct PendingWorker {
    pub dispatch: String,
    pub prompt: String,
    pub since: std::time::Instant,
}

impl MultisoulApp {
    pub(crate) fn receive_staff_tasks(&mut self, tasks: Vec<Task>, cx: &mut Context<Self>) {
        for task in tasks {
            if self
                .staff
                .tasks
                .get(&task.id)
                .is_none_or(|old| old.revision <= task.revision)
            {
                if self.staff.selected.as_deref() == Some(&task.id) {
                    let previous = self
                        .staff
                        .tasks
                        .get(&task.id)
                        .map_or(0, |old| old.messages.len());
                    let remaining = f32::from(self.staff.scroll.max_offset().height)
                        + f32::from(self.staff.scroll.offset().y);
                    // Use the existing assistant conversation's 48px bottom-follow threshold.
                    if task.messages.len() > previous
                        && (remaining <= 48.0
                            || matches!(
                                task.messages.last(),
                                Some(norma::protocol::TaskMessage::User { .. })
                            ))
                    {
                        self.staff
                            .scroll
                            .scroll_to_item(task.messages.len().saturating_sub(1));
                    }
                }
                self.staff.tasks.insert(task.id.clone(), task);
            }
        }
        self.staff.loaded = true;
        self.notify_staff(cx);
    }

    pub(crate) fn notify_staff(&self, cx: &mut Context<Self>) {
        cx.notify();
        if let Some(host) = self.desktop_host.clone() {
            cx.defer(move |cx| {
                let _ = host.update(cx, |_, cx| cx.notify());
            });
        }
    }
}

pub(crate) fn error_message(reason: String) -> ui::i18n::LocalizedText {
    let key = match reason.as_str() {
        "staff.invalid_source" => "staff.invalid_source",
        "staff.agent_unavailable" => "staff.agent_unavailable",
        "staff.proposal_unavailable" => "staff.proposal_unavailable",
        "staff.approval_required" => "staff.approval_required",
        "staff.worker_not_ready" => "staff.worker_not_ready",
        "staff.worker_failed" => "staff.worker_failed",
        "staff.connection_unavailable" => "staff.connection_unavailable",
        "island.staff_loading" => "island.staff_loading",
        "island.staff_disabled" => "island.staff_disabled",
        "island.staff_no_workspace" => "island.staff_no_workspace",
        _ => {
            tracing::warn!(%reason, "staff operation failed");
            "staff.operation_failed"
        }
    };
    ui::i18n::message(key, &[])
}

#[cfg(test)]
#[path = "../../tests/unit/staff_layout_tests.rs"]
mod layout_tests;
