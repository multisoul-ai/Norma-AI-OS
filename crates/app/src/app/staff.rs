//! Staff UI state is a cache; TaskStore is the authoritative writer.
use super::MultisoulApp;
use gpui::{Context, Entity};
use norma::protocol::Task;
use std::collections::BTreeMap;
use ui::component::input::InputState;
mod actions;
mod delivery;
pub(crate) use delivery::{
    delivery_action, delivery_confirmed, output_is_quiet, DeliveryAction, DeliveryStage,
};
#[cfg(test)]
pub(crate) use delivery::{
    AGENT_ENTER_DELAY, AGENT_OUTPUT_QUIET, AGENT_STARTUP_GRACE, WORKER_DELIVERY_CONFIRM,
};
mod activity;
mod appearance;
mod composer;
mod detail;
mod messages;
mod proposal_choice;
mod questions;
mod task_list;
mod view;
mod workspace;

/// 与原型相同：窄桌面压缩列表，保留对话宽度。
pub(crate) fn sidebar_width(window: &gpui::Window) -> f32 {
    let width = f32::from(window.viewport_size().width);
    if width < 760.0 {
        0.0
    } else {
        280.0
    }
}

#[derive(Default)]
pub(crate) struct StaffState {
    pub enabled: bool,
    pub fullscreen: bool,
    pub sidebar_open: bool,
    pub loaded: bool,
    pub loading: bool,
    pub busy: bool,
    pub tasks: BTreeMap<String, Task>,
    pub activity: BTreeMap<String, activity::TaskActivity>,
    pub mirroring_questions: bool,
    pub worker_answers: Vec<(
        norma::protocol::QuestionRef,
        Vec<norma::protocol::QuestionAnswer>,
    )>,
    pub question_subscriptions: BTreeMap<String, Vec<gpui::Subscription>>,
    pub question_editors: BTreeMap<String, crate::workspace::desktop::QuestionEditor>,
    pub expanded: std::collections::BTreeSet<String>,
    pub workspace_id: Option<String>,
    pub project_filter: Option<String>,
    pub target_project: Option<String>,
    pub task_projects: BTreeMap<String, (String, String)>,
    pub selected: Option<String>,
    pub drafts: BTreeMap<String, String>,
    /// 采纳前用户为提案改选的智能体类型；缺省沿用提案原值。
    pub proposal_agents: BTreeMap<(String, u64), String>,
    pub proposal_models: BTreeMap<(String, u64), Entity<InputState>>,
    pub input: Option<Entity<InputState>>,
    pub search: Option<Entity<InputState>>,
    pub query: String,
    pub filter: Option<norma::protocol::TaskDisplayStatus>,
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
    pub stage: DeliveryStage,
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
                for message in &task.messages {
                    if let norma::protocol::TaskMessage::Question { question } = message {
                        if question.state != norma::protocol::StaffQuestionState::Pending {
                            self.staff.question_editors.remove(&question.id);
                            self.staff.question_subscriptions.remove(&question.id);
                        }
                    }
                }
                if let Some(activity) = self.staff.activity.get_mut(&task.id) {
                    activity.reconcile(&task);
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
        "staff.proposal_superseded" => "staff.proposal_superseded",
        "staff.question_invalid" | "incomplete_answers" | "invalid_answer_cardinality" => {
            "staff.question_invalid"
        }
        "staff.question_unavailable" => "staff.question_unavailable",
        "staff.question_identity_changed" => "staff.question_identity_changed",
        "staff.invalid_source" => "staff.invalid_source",
        "staff.agent_unavailable" => "staff.agent_unavailable",
        "staff.proposal_unavailable" => "staff.proposal_unavailable",
        "staff.approval_required" => "staff.approval_required",
        "staff.worker_not_ready" | "staff.worker_needs_attention" => "staff.worker_needs_attention",
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

#[cfg(test)]
#[path = "../../tests/unit/staff_worker_delivery_tests.rs"]
mod delivery_tests;

#[cfg(test)]
#[path = "../../tests/unit/staff_card_tests.rs"]
mod card_tests;

#[cfg(test)]
#[path = "../../tests/unit/staff_keyboard_tests.rs"]
mod keyboard_tests;
