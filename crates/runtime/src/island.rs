//! GPUI-free dynamic island decisions. Callers provide monotonic milliseconds
//! and schedule only `deadline_ms()`; an idle island needs no polling timer.
pub mod approval_rules;
pub mod displays;
pub mod geometry;
pub mod transition;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IslandMode {
    #[default]
    Compact,
    Activity,
    Alert,
    Expanded,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IslandAlertKind {
    Approval,
    Question,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IslandAlert {
    /// Producer-supplied identity scoped by workspace, card and request.
    pub id: String,
    pub kind: IslandAlertKind,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IslandOverview {
    pub alerts: Vec<IslandAlert>,
    pub review_count: usize,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IslandTab {
    Pending,
    #[default]
    Running,
    Staff,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IslandPreferences {
    pub hidden: bool,
    pub approvals: bool,
    pub questions: bool,
    pub completions: bool,
}
impl Default for IslandPreferences {
    fn default() -> Self {
        Self {
            hidden: false,
            approvals: true,
            questions: true,
            completions: true,
        }
    }
}
#[derive(Clone, Debug)]
pub enum IslandEvent {
    PointerEntered,
    PointerLeft,
    Tick,
    Completed(String),
    /// A user action was acknowledged by the original request's transport.
    ActionConfirmed(String),
    OverviewUpdated(IslandOverview),
    ToggleExpanded,
    Expand,
    Escape,
    Located,
    TogglePin,
    InputFocusChanged(bool),
    SelectTab(IslandTab),
    SpeakingChanged(bool),
    PreferencesChanged(IslandPreferences),
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IslandState {
    mode: IslandMode,
    pointer_inside: bool,
    deadline: Option<u64>,
    activity_id: Option<String>,
    alert_id: Option<String>,
    overview: IslandOverview,
    confirmed_alerts: std::collections::BTreeSet<String>,
    pinned: bool,
    input_focused: bool,
    speaking: bool,
    tab: IslandTab,
    preferences: IslandPreferences,
}
impl IslandState {
    pub fn mode(&self) -> IslandMode {
        self.mode
    }
    pub fn preferences(&self) -> IslandPreferences {
        self.preferences
    }
    pub fn pinned(&self) -> bool {
        self.pinned
    }
    pub fn speaking(&self) -> bool {
        self.speaking
    }
    pub fn tab(&self) -> IslandTab {
        self.tab
    }
    pub fn alert_id(&self) -> Option<&str> {
        self.alert_id.as_deref()
    }
    pub fn activity_id(&self) -> Option<&str> {
        self.activity_id.as_deref()
    }
    /// A receipt hides the old projection until the producer removes that exact identity.
    pub fn is_confirmed(&self, id: &str) -> bool {
        self.confirmed_alerts.contains(id)
    }
    pub fn deadline_ms(&self) -> Option<u64> {
        self.deadline
    }
    /// Returns whether state changed; callers can avoid redundant UI notifications.
    pub fn apply(&mut self, event: IslandEvent, now_ms: u64) -> bool {
        let before = self.clone();
        match event {
            IslandEvent::PreferencesChanged(preferences) => {
                self.preferences = preferences;
                if preferences.hidden {
                    self.collapse();
                    self.pointer_inside = false;
                } else if self.mode == IslandMode::Activity
                    && self.activity_id.is_some()
                    && !preferences.completions
                {
                    self.collapse();
                } else if self.mode == IslandMode::Alert {
                    let id = self
                        .overview
                        .alerts
                        .iter()
                        .find(|a| Some(&a.id) == self.alert_id.as_ref() && self.alert_enabled(a))
                        .or_else(|| self.overview.alerts.iter().find(|a| self.alert_enabled(a)))
                        .map(|a| a.id.clone());
                    if id.is_some() {
                        self.alert_id = id;
                    } else {
                        self.collapse();
                    }
                }
            }
            event if self.preferences.hidden => match event {
                IslandEvent::OverviewUpdated(overview) => self.overview = overview,
                IslandEvent::SpeakingChanged(speaking) => self.speaking = speaking,
                _ => {}
            },
            IslandEvent::PointerEntered if !self.pointer_inside => {
                self.pointer_inside = true;
                if self.mode == IslandMode::Compact {
                    self.deadline = Some(now_ms.saturating_add(160));
                } else if self.mode == IslandMode::Expanded {
                    self.deadline = None;
                }
            }
            IslandEvent::PointerLeft if self.pointer_inside => {
                self.pointer_inside = false;
                match self.mode {
                    IslandMode::Expanded => self.arm_collapse(now_ms),
                    IslandMode::Compact => self.deadline = None,
                    _ => {}
                }
            }
            IslandEvent::Tick if self.deadline.is_some_and(|at| now_ms >= at) => {
                if self.mode == IslandMode::Compact && self.pointer_inside {
                    self.expand();
                } else {
                    self.collapse();
                }
            }
            IslandEvent::Completed(id)
                if self.preferences.completions
                    && matches!(self.mode, IslandMode::Compact | IslandMode::Activity) =>
            {
                self.mode = IslandMode::Activity;
                self.activity_id = Some(id);
                self.deadline = Some(now_ms.saturating_add(3000));
            }
            IslandEvent::ActionConfirmed(id) => {
                self.confirmed_alerts.insert(id);
                if self.mode != IslandMode::Expanded {
                    self.alert_id = self
                        .overview
                        .alerts
                        .iter()
                        .find(|a| self.alert_enabled(a))
                        .map(|a| a.id.clone());
                    self.activity_id = None;
                    if self.alert_id.is_some() {
                        self.mode = IslandMode::Alert;
                        self.deadline = None;
                    } else {
                        self.mode = IslandMode::Activity;
                        self.deadline = Some(now_ms.saturating_add(2200));
                    }
                }
            }
            IslandEvent::OverviewUpdated(overview) => {
                self.confirmed_alerts
                    .retain(|id| overview.alerts.iter().any(|a| &a.id == id));
                let next = if self.mode == IslandMode::Alert {
                    overview
                        .alerts
                        .iter()
                        .find(|a| Some(&a.id) == self.alert_id.as_ref() && self.alert_enabled(a))
                        .or_else(|| overview.alerts.iter().find(|a| self.alert_enabled(a)))
                } else {
                    overview.alerts.iter().find(|a| {
                        self.alert_enabled(a)
                            && !self.overview.alerts.iter().any(|old| old.id == a.id)
                    })
                }
                .map(|a| a.id.clone());
                if self.mode != IslandMode::Expanded {
                    if let Some(id) = next {
                        self.mode = IslandMode::Alert;
                        self.alert_id = Some(id);
                        self.activity_id = None;
                        self.deadline = None;
                    } else if self.mode == IslandMode::Alert {
                        self.mode = IslandMode::Compact;
                        self.alert_id = None;
                    }
                }
                self.overview = overview;
            }
            IslandEvent::Expand => self.expand(),
            IslandEvent::ToggleExpanded if self.mode == IslandMode::Expanded => self.collapse(),
            IslandEvent::ToggleExpanded => self.expand(),
            IslandEvent::Escape | IslandEvent::Located => self.collapse(),
            IslandEvent::TogglePin if self.mode == IslandMode::Expanded => {
                self.pinned = !self.pinned;
                self.arm_collapse(now_ms);
            }
            IslandEvent::InputFocusChanged(focused)
                if matches!(self.mode, IslandMode::Expanded | IslandMode::Alert) =>
            {
                if self.input_focused != focused {
                    self.input_focused = focused;
                    if self.mode == IslandMode::Expanded {
                        self.arm_collapse(now_ms);
                    }
                }
            }
            IslandEvent::SelectTab(tab) if self.mode == IslandMode::Expanded => self.tab = tab,
            IslandEvent::SpeakingChanged(speaking) => self.speaking = speaking,
            _ => {}
        }
        *self != before
    }
    fn alert_enabled(&self, alert: &IslandAlert) -> bool {
        if self.confirmed_alerts.contains(&alert.id) {
            return false;
        }
        match alert.kind {
            IslandAlertKind::Approval => self.preferences.approvals,
            IslandAlertKind::Question => self.preferences.questions,
        }
    }
    fn arm_collapse(&mut self, now_ms: u64) {
        self.deadline = (!self.pointer_inside && !self.pinned && !self.input_focused)
            .then(|| now_ms.saturating_add(420));
    }
    fn expand(&mut self) {
        if self.mode == IslandMode::Expanded {
            return;
        }
        self.mode = IslandMode::Expanded;
        self.tab = if self.speaking {
            IslandTab::Staff
        } else if self
            .overview
            .alerts
            .iter()
            .any(|a| !self.is_confirmed(&a.id))
            || self.overview.review_count > 0
        {
            IslandTab::Pending
        } else {
            IslandTab::Running
        };
        self.deadline = None;
        self.alert_id = None;
        self.activity_id = None;
    }
    fn collapse(&mut self) {
        self.mode = IslandMode::Compact;
        self.pinned = false;
        self.input_focused = false;
        self.deadline = None;
        self.alert_id = None;
        self.activity_id = None;
    }
}
#[cfg(test)]
#[path = "../tests/unit/island_timing.rs"]
mod timing_tests;

#[cfg(test)]
#[path = "../tests/unit/island_attention.rs"]
mod attention_tests;

#[cfg(test)]
#[path = "../tests/unit/island_interaction.rs"]
mod interaction_tests;

#[cfg(test)]
#[path = "../tests/unit/island_preferences.rs"]
mod preferences_tests;

/// Completion popups originate only from a new successful task transition.
pub fn completion_node(event: &crate::RuntimeEvent) -> Option<u64> {
    match event {
        crate::RuntimeEvent::AgentTaskStateChanged {
            node_id,
            from,
            to: crate::AgentTaskState::Done,
            ..
        } if *from != crate::AgentTaskState::Done => Some(*node_id),
        _ => None,
    }
}
