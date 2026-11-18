//! Staff UI state is a cache; TaskStore is the authoritative writer.
use super::MultisoulApp;
use gpui::{Context, Entity};
use norma::protocol::Task;
use std::collections::BTreeMap;
use ui::component::input::InputState;
mod actions;
mod archive;
mod delivery;
pub(crate) use delivery::{
    delivery_action, delivery_confirmed, output_is_quiet, quiet_allows_typing, DeliveryAction,
    DeliveryStage,
};
#[cfg(test)]
pub(crate) use delivery::{
    emits_ready_signal, AGENT_ENTER_DELAY, AGENT_OUTPUT_QUIET, AGENT_STARTUP_GRACE,
    WORKER_DELIVERY_CONFIRM, WORKER_ENTER_RETRY,
};
mod activity;
mod appearance;
mod composer;
mod detail;
mod images;
mod messages;
mod open_link;
pub(crate) use open_link::{install_link_handler, register_link_target};
mod list_type;
mod proposal_choice;
mod questions;
mod task_list;
mod task_row;
mod task_time;
mod view;
mod worker_remove;
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
    pub images: Vec<String>,
    pub image_drafts: BTreeMap<String, Vec<String>>,
    pub image_uploads: u8,
    /// 采纳前用户为提案改选的智能体类型；缺省沿用提案原值。
    pub proposal_agents: BTreeMap<(String, u64), String>,
    /// 采纳前选定的模型 id；缺省表示用 CLI 默认。
    pub proposal_models: BTreeMap<(String, u64), String>,
    /// 已为模型下拉触发过后端模型列表读取。
    pub models_probed: bool,
    pub input: Option<Entity<InputState>>,
    pub search: Option<Entity<InputState>>,
    pub query: String,
    pub filter: Option<norma::protocol::TaskDisplayStatus>,
    pub archive_view: bool,
    pub toast: Option<StaffToast>,
    pub remove_ask: Option<WorkerRemoveAsk>,
    pub toast_generation: u64,
    pub scroll: gpui::ScrollHandle,
    /// 贴底跟随中：内容之后变高（表格换行、流式文字转 Markdown）时补滚到底；用户上翻即退出。
    pub pinned: bool,
    pub error: Option<ui::i18n::LocalizedText>,
    pub pending: BTreeMap<u64, PendingWorker>,
    /// 已判「任务还没送达」的 (节点 → dispatch)：确认晚到时据此收回，重新投递或换派发即清除。
    pub unconfirmed: BTreeMap<u64, String>,
    pub events: std::cell::RefCell<std::collections::VecDeque<brain::BrainEvent>>,
    pub saving_event: bool,
    pub recorded: std::cell::RefCell<
        std::collections::VecDeque<(brain::BrainEvent, brain::staff::TaskEventResult)>,
    >,
}

#[derive(Clone)]
pub(crate) struct StaffToast {
    pub text: ui::i18n::LocalizedText,
    pub undo_task: Option<String>,
    /// 提示条上「撤销」恢复的 Worker：(任务号, dispatch_id)。
    pub undo_worker: Option<(String, String)>,
    pub generation: u64,
}

/// 用户点了移除，但 Worker 还在执行：等确认。
#[derive(Clone)]
pub(crate) struct WorkerRemoveAsk {
    pub task_id: String,
    pub dispatch_id: String,
    pub agent: String,
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
                    // 贴底跟随：只有原本就在底部附近、或用户刚发了消息时才自动滚动。
                    if task.messages.len() > previous
                        && (self.staff_near_bottom()
                            || matches!(
                                task.messages.last(),
                                Some(norma::protocol::TaskMessage::User { .. })
                            ))
                    {
                        self.staff.pinned = true;
                        self.staff.scroll.scroll_to_bottom();
                    }
                }
                for question in &task.questions {
                    if question.state != norma::protocol::StaffQuestionState::Pending {
                        self.staff.question_editors.remove(&question.id);
                        self.staff.question_subscriptions.remove(&question.id);
                    }
                }
                self.staff.tasks.insert(task.id.clone(), task);
            }
        }
        self.staff.loaded = true;
        self.notify_staff(cx);
    }

    /// 流式增量：只替换本轮那条回复，不碰 revision；落盘后的完整任务会覆盖它。
    pub(crate) fn receive_staff_reply(
        &mut self,
        task_id: &str,
        index: usize,
        reply: norma::protocol::StaffReply,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = self
            .staff
            .tasks
            .get_mut(task_id)
            .and_then(|task| task.messages.get_mut(index))
        else {
            return;
        };
        let norma::protocol::TaskMessage::Staff(old) = slot else {
            return;
        };
        *old = reply;
        // 只看 pinned，不看 near_bottom。上翻 30px 仍在 48px 内，下一块不能把视图拽回去。
        if self.staff.selected.as_deref() == Some(task_id) && self.staff.pinned {
            self.staff.scroll.scroll_to_bottom();
        }
        self.notify_staff(cx);
    }

    /// 与桌面助手对话相同的 48px 贴底阈值。
    pub(crate) fn staff_near_bottom(&self) -> bool {
        f32::from(self.staff.scroll.max_offset().height) + f32::from(self.staff.scroll.offset().y)
            <= 48.0
    }

    /// 滚轮监听发生在 GPUI 改偏移之前。上翻退出跟随；下翻用这一笔之后的位置决定是否恢复。
    pub(crate) fn staff_note_scroll_wheel(&mut self, delta_y: gpui::Pixels) {
        if delta_y > gpui::px(0.) {
            self.staff.pinned = false;
        } else if delta_y < gpui::px(0.) {
            let max_h = self.staff.scroll.max_offset().height;
            let next = (self.staff.scroll.offset().y + delta_y).clamp(-max_h, gpui::px(0.));
            if f32::from(max_h) + f32::from(next) <= 48.0 {
                self.staff.pinned = true;
            }
        }
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
    let (code, engines) = reason.split_once('\n').unwrap_or((reason.as_str(), ""));
    let key = match code {
        "staff.proposal_superseded" => "staff.proposal_superseded",
        "staff.question_invalid" | "incomplete_answers" | "invalid_answer_cardinality" => {
            "staff.question_invalid"
        }
        "staff.question_unavailable" => "staff.question_unavailable",
        "staff.question_identity_changed" => "staff.question_identity_changed",
        "staff.invalid_source" => "staff.invalid_source",
        "staff.denied.decide" => "staff.denied.decide",
        "staff.denied.answer" => "staff.denied.answer",
        "staff.denied.legacy_assign" => "staff.denied.legacy_assign",
        "staff.denied.task_scope" => "staff.denied.task_scope",
        "staff.denied.reply" => "staff.denied.reply",
        "staff.denied.user_message" => "staff.denied.user_message",
        "staff.denied.adopt" => "staff.denied.adopt",
        "staff.denied.remove_worker" => "staff.denied.remove_worker",
        "staff.denied.unarchive" => "staff.denied.unarchive",
        "staff.denied.archive_not_done" => "staff.denied.archive_not_done",
        "staff.denied.other_task" => "staff.denied.other_task",
        "staff.denied.history_scope" => "staff.denied.history_scope",
        "staff.denied.report_rate" => "staff.denied.report_rate",
        "staff.agent_unavailable" => "staff.agent_unavailable",
        "staff.worker_card_gone" => "staff.worker_card_gone",
        "staff.proposal_unavailable" => "staff.proposal_unavailable",
        "staff.approval_required" => "staff.approval_required",
        "staff.worker_not_ready" | "staff.worker_needs_attention" => "staff.worker_needs_attention",
        "staff.worker_failed" => "staff.worker_failed",
        "staff.worker_cancelled" => "staff.worker_cancelled",
        "staff.connection_unavailable" => "staff.connection_unavailable",
        "invalid_image" => "staff.image_invalid",
        "too_many_images" => "staff.image_too_many",
        "image_storage_full" => "staff.image_storage_full",
        "image_storage_unavailable" => "staff.image_unavailable",
        "island.staff_loading" => "island.staff_loading",
        "island.staff_disabled" => "island.staff_disabled",
        "island.staff_no_workspace" => "island.staff_no_workspace",
        _ => {
            tracing::warn!(%reason, "staff operation failed");
            "staff.operation_failed"
        }
    };
    if key == "staff.agent_unavailable" && !engines.is_empty() {
        return ui::i18n::message(
            "staff.agent_unavailable_choices",
            &[("engines", engines.to_owned())],
        );
    }
    ui::i18n::message(key, &[])
}

#[cfg(test)]
#[path = "../../tests/unit/staff_task_time_tests.rs"]
mod task_time_tests;

#[cfg(test)]
#[path = "../../tests/unit/staff_layout_tests.rs"]
mod layout_tests;

#[cfg(test)]
#[path = "../../tests/unit/staff_layout_wrap_tests.rs"]
mod layout_wrap_tests;

#[cfg(test)]
#[path = "../../tests/unit/staff_worker_delivery_tests.rs"]
mod delivery_tests;

#[cfg(test)]
#[path = "../../tests/unit/staff_card_tests.rs"]
mod card_tests;

#[cfg(test)]
#[path = "../../tests/unit/staff_keyboard_tests.rs"]
mod keyboard_tests;

#[cfg(test)]
#[path = "../../tests/unit/staff_content_layout_tests.rs"]
mod content_layout_tests;

#[cfg(test)]
#[path = "../../tests/unit/staff_rich_reply_tests.rs"]
mod rich_reply_tests;

#[cfg(test)]
#[path = "../../tests/unit/staff_reply_tests.rs"]
mod reply_tests;

#[cfg(test)]
#[path = "../../tests/unit/staff_timeline_flow_tests.rs"]
mod timeline_flow_tests;

#[cfg(test)]
#[path = "../../tests/unit/staff_wide_column_tests.rs"]
mod wide_column_tests;
