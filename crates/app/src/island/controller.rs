use super::{IslandModel, IslandWorkspace};
use gpui::{Context, Entity, Subscription, Task};
use runtime::island::{displays::IslandDisplays, IslandEvent, IslandOverview};
use std::time::{Duration, Instant};

pub(super) struct IslandController {
    pub displays: IslandDisplays,
    pub model: Entity<IslandModel>,
    pub message: Option<ui::i18n::LocalizedText>,
    pub completion_label: Option<String>,
    pub pending: std::collections::BTreeSet<String>,
    pub checked_grants: std::collections::BTreeSet<String>,
    pub navigation: u64,
    pub instant_revision: u64,
    pub idle_open: std::collections::BTreeSet<String>,
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
            pending: Default::default(),
            checked_grants: Default::default(),
            navigation: 0,
            instant_revision: 0,
            idle_open: Default::default(),
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
        self.event(
            self.displays.active_display().unwrap_or_default(),
            if confirmed {
                IslandEvent::ActionConfirmed(id)
            } else {
                IslandEvent::Expand
            },
            cx,
        );
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
                overview.review_count += project
                    .overview
                    .attention
                    .iter()
                    .filter(|a| {
                        matches!(
                            a.kind,
                            norma::mobile::RunOverviewAttentionKind::Review
                                | norma::mobile::RunOverviewAttentionKind::File
                        )
                    })
                    .count();
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
    workspaces
        .iter()
        .flat_map(|ws| &ws.projects)
        .fold((0, 0), |(running, pending), p| {
            (
                running + p.overview.running.len(),
                pending
                    + p.approvals.len()
                    + p.questions.len()
                    + p.overview
                        .attention
                        .iter()
                        .filter(|a| {
                            matches!(
                                a.kind,
                                norma::mobile::RunOverviewAttentionKind::Review
                                    | norma::mobile::RunOverviewAttentionKind::File
                            )
                        })
                        .count(),
            )
        })
}

#[cfg(test)]
#[path = "../../tests/unit/island_controller_tests.rs"]
mod tests;
