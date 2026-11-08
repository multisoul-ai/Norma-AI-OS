use super::{IslandModel, IslandWorkspace};
use gpui::{Context, Entity, Subscription, Task};
use runtime::island::{displays::IslandDisplays, IslandEvent, IslandOverview};
use std::time::{Duration, Instant};

pub(super) struct IslandController {
    pub displays: IslandDisplays,
    pub model: Entity<IslandModel>,
    pub message: Option<ui::i18n::LocalizedText>,
    pub completion_label: Option<String>,
    pub completion_route: Option<super::CompletionRoute>,
    pub pending: std::collections::BTreeSet<String>,
    pub checked_grants: std::collections::BTreeSet<String>,
    pub navigation: u64,
    pub instant_revision: u64,
    pub run_filter: runtime::island::roster::RunFilter,
    pub focused_review: Option<String>,
    queue_ids: Vec<String>,
    pub editing: std::collections::BTreeMap<String, super::question_editor::EditingQuestion>,
    pub focused_question: Option<(String, gpui::EntityId)>,
    pub(super) gate: runtime::island::notification::NotificationGate,
    pub(super) post: Option<super::fallback::Post>,
    screens: Vec<u32>,
    epoch: Instant,
    timer: Option<Task<()>>,
    _subscription: Subscription,
    _completion_subscription: Subscription,
    _settings_subscription: Option<Subscription>,
}

impl IslandController {
    pub fn new(model: Entity<IslandModel>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&model, |this, _, cx| {
            this.refresh(cx);
            cx.notify();
        });
        let completion_subscription =
            cx.subscribe(&model, |this, _, notice: &super::CompletionNotice, cx| {
                this.completion_label = Some(notice.label.clone());
                this.completion_route = notice.route.clone();
                this.notify_completed(&notice.label, cx);
                this.event(
                    this.displays.active_display().unwrap_or_default(),
                    IslandEvent::Completed(notice.id.clone()),
                    cx,
                );
            });
        let settings_subscription = super::preferences::state(cx).map(|settings| {
            cx.observe(&settings, |this, settings, cx| {
                let preferences = settings.read(cx).preferences;
                this.notify_hidden(cx);
                this.event(
                    this.displays.active_display().unwrap_or_default(),
                    IslandEvent::PreferencesChanged(preferences),
                    cx,
                );
            })
        });
        let mut this = Self {
            displays: IslandDisplays::default(),
            model,
            message: None,
            completion_label: None,
            completion_route: None,
            pending: Default::default(),
            checked_grants: Default::default(),
            navigation: 0,
            instant_revision: 0,
            run_filter: runtime::island::roster::RunFilter::All,
            focused_review: None,
            queue_ids: Vec::new(),
            editing: Default::default(),
            focused_question: None,
            gate: Default::default(),
            post: None,
            screens: Vec::new(),
            epoch: Instant::now(),
            timer: None,
            _subscription: subscription,
            _completion_subscription: completion_subscription,
            _settings_subscription: settings_subscription,
        };
        this.displays.apply(
            0,
            IslandEvent::PreferencesChanged(super::preferences::current(cx)),
            0,
        );
        this.install_notifier(cx);
        this.refresh(cx);
        this
    }

    pub fn event(&mut self, display: u32, event: IslandEvent, cx: &mut Context<Self>) {
        let now = self.epoch.elapsed().as_millis() as u64;
        let tick = matches!(event, IslandEvent::Tick);
        let completed = match &event {
            IslandEvent::Completed(id) => Some(id.clone()),
            _ => None,
        };
        let immediate = matches!(
            event,
            IslandEvent::Escape | IslandEvent::ToggleExpanded | IslandEvent::Located
        );
        let previous_display = self.displays.active_display();
        let previous_mode = self.displays.state.mode();
        let previous_alert = self.displays.state.alert_id().map(str::to_owned);
        let changed = self.displays.apply(display, event, now);
        let collapsed = self.displays.state.mode() == runtime::island::IslandMode::Compact
            && previous_mode != runtime::island::IslandMode::Compact;
        let new_alert = self.displays.state.alert_id().is_some()
            && self.displays.state.alert_id() != previous_alert.as_deref();
        if collapsed
            || new_alert
            || completed
                .as_deref()
                .is_some_and(|id| self.displays.state.activity_id() == Some(id))
        {
            self.message = None;
        }
        if previous_display != self.displays.active_display() {
            self.focused_question = None;
        }
        if immediate {
            self.instant_revision = self.instant_revision.wrapping_add(1);
        }
        self.arm_timer(cx);
        if changed || immediate || (tick && self.clock_visible(cx)) {
            cx.notify();
        }
    }

    pub fn retain_displays(&mut self, displays: &[u32], cx: &mut Context<Self>) {
        self.screens = displays.to_vec();
        self.displays
            .retain_displays(displays, self.epoch.elapsed().as_millis() as u64);
        self.arm_timer(cx);
        cx.notify();
    }

    pub fn action_feedback(&mut self, id: String, confirmed: bool, cx: &mut Context<Self>) {
        let focus = self.focused_review.clone().unwrap_or_else(|| id.clone());
        self.event(
            self.displays.active_display().unwrap_or_default(),
            if confirmed {
                IslandEvent::ActionConfirmed(id)
            } else {
                IslandEvent::Expand
            },
            cx,
        );
        if confirmed {
            self.follow_queue(Some(focus), cx);
        }
    }

    /// Shared presentation snapshot for counts, rows and input mounting.
    pub fn visible_workspaces(&self, cx: &gpui::App) -> Vec<IslandWorkspace> {
        let mut workspaces = self.model.read(cx).workspaces.clone();
        for workspace in &mut workspaces {
            for project in &mut workspace.projects {
                let source = project.source;
                let area = &project.area_id;
                project.approvals.retain(|row| {
                    !self.displays.state.is_confirmed(&alert_key(
                        source,
                        &workspace.id,
                        area,
                        &row.target,
                    ))
                });
                project.questions.retain(|row| {
                    !self.displays.state.is_confirmed(&alert_key(
                        source,
                        &workspace.id,
                        area,
                        &row.snapshot.question.target,
                    ))
                });
            }
        }
        workspaces
    }

    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.check_remembered(cx);
        let mut overview = IslandOverview {
            alerts: self.current_alerts(cx),
            ..Default::default()
        };
        for workspace in &self.model.read(cx).workspaces {
            for project in &workspace.projects {
                overview.review_count +=
                    super::faces::result_cards(&project.overview.attention).len();
            }
        }
        self.notify_hidden(cx);
        self.editing
            .retain(|id, _| overview.alerts.iter().any(|alert| &alert.id == id));
        if let Some(id) = self.focused_question.as_ref().map(|(id, _)| id.clone()) {
            if !overview.alerts.iter().any(|alert| alert.id == id) {
                self.release_question_focus(&id, cx);
            }
        }
        let previous_focus = self
            .focused_review
            .clone()
            .or_else(|| self.displays.state.alert_id().map(str::to_owned));
        let pointer_display = (!self.screens.is_empty())
            .then(super::native::pointer_screen)
            .flatten();
        self.event(
            pointer_display
                .filter(|id| self.screens.contains(id))
                .or(self.displays.active_display())
                .unwrap_or_default(),
            IslandEvent::OverviewUpdated(overview),
            cx,
        );
        self.follow_queue(previous_focus, cx);
    }

    /// After a card leaves, show the next approval, question or result.
    fn follow_queue(&mut self, previous_focus: Option<String>, cx: &mut Context<Self>) {
        let workspaces = self.visible_workspaces(cx);
        let items = super::faces::queue_items(&workspaces);
        let step = super::faces::queue_step(&self.queue_ids, &items, previous_focus.as_deref());
        let still_here = previous_focus
            .as_ref()
            .is_some_and(|id| items.iter().any(|item| &item.id == id));
        self.queue_ids = items.into_iter().map(|item| item.id).collect();
        if still_here
            || !matches!(
                self.displays.state.mode(),
                runtime::island::IslandMode::Alert | runtime::island::IslandMode::Activity
            )
        {
            return;
        }
        let display = self.displays.active_display().unwrap_or_default();
        if step.hold_review {
            let changed = self.focused_review != step.focused_review;
            self.focused_review = step.focused_review;
            if changed {
                self.message = None;
            }
            self.event(display, IslandEvent::ReviewHold(true), cx);
        } else if let Some(id) = step.select_alert {
            self.focused_review = None;
            self.event(display, IslandEvent::SelectAlert(id), cx);
        }
    }

    fn clock_visible(&self, cx: &gpui::App) -> bool {
        self.displays.state.mode() == runtime::island::IslandMode::Expanded
            && self.displays.state.tab() == runtime::island::IslandTab::Running
            && self
                .model
                .read(cx)
                .workspaces
                .iter()
                .any(|ws| ws.projects.iter().any(|p| !p.overview.running.is_empty()))
    }

    fn arm_timer(&mut self, cx: &mut Context<Self>) {
        self.timer = None;
        let now = self.epoch.elapsed().as_millis() as u64;
        let clock = self.clock_visible(cx).then_some(now.saturating_add(1000));
        let Some(deadline) = self
            .displays
            .state
            .deadline_ms()
            .into_iter()
            .chain(clock)
            .min()
        else {
            return;
        };
        let remaining = deadline.saturating_sub(now);
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(remaining))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.event(
                    this.displays.active_display().unwrap_or_default(),
                    IslandEvent::Tick,
                    cx,
                );
            });
        }));
    }
}

pub(super) fn alert_key(
    source: u64,
    workspace: &str,
    area: &str,
    request: &impl serde::Serialize,
) -> String {
    serde_json::to_string(&(source, workspace, area, request))
        .expect("island protocol identity is serializable")
}

pub(super) fn counts(workspaces: &[IslandWorkspace]) -> (usize, usize) {
    // The running tab matches faces still working. A finished agent can remain
    // in the overview running list until the review is cleared.
    let running = super::faces::collect(workspaces)
        .iter()
        .filter(|face| face.activity == runtime::island::roster::AgentActivity::Running)
        .count();
    let pending = workspaces
        .iter()
        .flat_map(|workspace| &workspace.projects)
        .fold(0, |pending, project| {
            pending
                + project.approvals.len()
                + project.questions.len()
                + super::faces::result_cards(&project.overview.attention).len()
        });
    (running, pending)
}

#[cfg(test)]
#[path = "../../tests/unit/island_controller_tests.rs"]
mod tests;
