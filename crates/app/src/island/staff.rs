//! Global island staff chat: one transcript for every workspace, anchored on a live host only to
//! reach the single shared brain. Replies come back through `norma brain report --kind reply`.
use super::IslandWorkspace;
use gpui::{App, AppContext, Entity, Global};
use norma::mobile::RunOverviewAttentionKind;
use runtime::island::staff::{
    bounded_overview, split_sentences, OverviewEntry, StaffChat, StaffReply,
};
use std::time::Instant;

#[cfg(test)]
#[path = "../../tests/unit/island_staff_tests.rs"]
mod tests;

/// Budget for the overview handed to the brain; it can read more with `history`/`context`.
const OVERVIEW_CHARS: usize = 3000;
const LINE_CHARS: usize = 120;

#[derive(Default)]
pub(crate) struct StaffState {
    pub chat: StaffChat,
    speech: Option<(Vec<String>, Instant)>,
    pub activity: Vec<crate::app::ActivityLine>,
    activity_loading: bool,
    activity_at: Option<Instant>,
    pub notice: Option<ui::i18n::LocalizedText>,
    /// When the last user-requested report arrived; shown as "last report … ago".
    pub last_report: Option<Instant>,
}

struct SharedStaff(Entity<StaffState>);
impl Global for SharedStaff {}

pub(crate) fn state(cx: &mut App) -> Entity<StaffState> {
    if let Some(global) = cx.try_global::<SharedStaff>() {
        return global.0.clone();
    }
    let state = cx.new(|_| StaffState::default());
    cx.set_global(SharedStaff(state.clone()));
    state
}

impl StaffState {
    pub fn begin_report(&mut self) -> bool {
        self.chat.request_report(now_ms())
    }

    pub fn report_pending(&self) -> bool {
        self.chat.report_pending(now_ms())
    }

    /// Records a brain reply; an explicit report also starts per-sentence captions.
    pub fn deliver_reply(&mut self, text: &str) -> Option<StaffReply> {
        let reply = self.chat.push_reply(text, now_ms())?;
        if reply.speak {
            self.last_report = Some(Instant::now());
        }
        self.speech = reply.speak.then(|| (split_sentences(text), Instant::now()));
        self.notice = None;
        Some(reply)
    }

    /// The host reports whether the reply really entered the speech queue; without speech there is
    /// nothing to caption, so drop the captions and say the reply is text only.
    pub fn end_speech(&mut self, queued: bool) {
        if !queued && self.speech.take().is_some() {
            self.notice = Some(ui::i18n::LocalizedText::new(
                "island.staff_speech_unavailable",
            ));
        }
    }

    #[cfg(test)]
    pub fn caption_sentences(&self) -> &[String] {
        self.speech.as_ref().map_or(&[], |(sentences, _)| sentences)
    }

    /// The sentence being spoken now, `None` once the estimated speech has ended.
    pub fn caption(&self) -> Option<&str> {
        let (sentences, started) = self.speech.as_ref()?;
        let index =
            runtime::island::staff::caption_index(sentences, started.elapsed().as_millis() as u64)?;
        sentences.get(index).map(String::as_str)
    }
}

/// Hands the event to the first live host that can reach the brain; a failure names the most
/// useful reason (a disabled brain beats "no workspace").
fn anchor_push(kind: brain::BrainEventKind, cx: &mut App) -> Result<(), &'static str> {
    let mut reason = "island.staff_no_workspace";
    for host in super::actions::hosts(cx) {
        match host.update(cx, |app, _| app.push_staff_event(kind.clone())) {
            Ok(()) => return Ok(()),
            Err("island.staff_no_workspace") => {}
            Err(other) => reason = other,
        }
    }
    Err(reason)
}

fn record(result: Result<(), &'static str>, cx: &mut App, on_ok: impl FnOnce(&mut StaffState)) {
    state(cx).update(cx, |staff, cx| {
        match result {
            Ok(()) => {
                staff.notice = None;
                on_ok(staff);
            }
            Err(key) => staff.notice = Some(ui::i18n::LocalizedText::new(key)),
        }
        cx.notify();
    });
}

/// `Ok` when some live host can reach the brain; otherwise the most useful localized reason.
pub(super) fn availability(cx: &App) -> Result<(), &'static str> {
    let mut reason = "island.staff_no_workspace";
    for host in super::actions::hosts(cx) {
        match host.read(cx).staff_ready() {
            Ok(()) => return Ok(()),
            Err("island.staff_no_workspace") => {}
            Err(other) => reason = other,
        }
    }
    Err(reason)
}

/// A user message to the global staff; the reply arrives later through `report --kind reply`.
/// `true` when the brain accepted the message; otherwise the reason is shown and the caller keeps
/// the user's text.
pub(crate) fn send_message(text: &str, cx: &mut App) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    let result = anchor_push(
        brain::BrainEventKind::StaffRequest {
            text: text.to_owned(),
        },
        cx,
    );
    let accepted = result.is_ok();
    record(result, cx, |staff| {
        staff.chat.push_user(text);
    });
    accepted
}

/// The report button: a bounded cross-workspace overview is captured now (cached model data, no
/// IO) and handed to the brain, whose reply is spoken even when spoken reports are off.
pub(super) fn request_report(workspaces: &[IslandWorkspace], cx: &mut App) {
    let Some(overview) = report_overview(workspaces) else {
        record(Err("island.staff_nothing_to_report"), cx, |_| {});
        return;
    };
    if !state(cx).update(cx, |staff, _| staff.begin_report()) {
        return;
    }
    let result = anchor_push(brain::BrainEventKind::ReportRequested { overview }, cx);
    if result.is_err() {
        state(cx).update(cx, |staff, _| staff.chat.cancel_report());
    }
    record(result, cx, |_| {});
}

const ACTIVITY_ROWS: usize = 4;
const ACTIVITY_REFRESH: std::time::Duration = std::time::Duration::from_secs(3);

/// Reads the newest brain turns on the background executor; at most one read in flight and one
/// per refresh interval, so rendering the tab never touches SQLite.
pub(super) fn refresh_activity(cx: &mut App) {
    let shared = state(cx);
    let due = shared.update(cx, |staff, _| {
        let due = !staff.activity_loading
            && staff
                .activity_at
                .is_none_or(|at| at.elapsed() >= ACTIVITY_REFRESH);
        if due {
            staff.activity_loading = true;
            staff.activity_at = Some(Instant::now());
        }
        due
    });
    if !due {
        return;
    }
    let work = cx.background_executor().spawn(async {
        let paths = crate::database::default_database_paths().map_err(|e| e.to_string())?;
        crate::app::recent_activity_lines(paths.app_db_path().to_path_buf(), ACTIVITY_ROWS)
            .map_err(|e| e.to_string())
    });
    cx.spawn(async move |cx| {
        let result = work.await;
        let _ = shared.update(cx, |staff, cx| {
            staff.activity_loading = false;
            match result {
                Ok(lines) if lines != staff.activity => {
                    staff.activity = lines;
                    cx.notify();
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "读取岛内幕僚活动失败"),
            }
        });
    })
    .detach();
}

fn now_ms() -> u64 {
    use std::sync::OnceLock;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

fn clip(text: &str) -> String {
    let mut clipped: String = text.chars().take(LINE_CHARS).collect();
    if text.chars().count() > LINE_CHARS {
        clipped.push('…');
    }
    clipped
}

/// One entry per failure, takeover, running agent: the bounded input of a report request.
pub(super) fn overview_entries(workspaces: &[IslandWorkspace]) -> Vec<OverviewEntry> {
    let mut entries = Vec::new();
    for workspace in workspaces {
        for project in &workspace.projects {
            let mut push = |line: String| {
                entries.push(OverviewEntry {
                    workspace: workspace.name.clone(),
                    project: project.name.clone(),
                    line: clip(&line),
                });
            };
            for item in &project.overview.attention {
                let label = match item.kind {
                    RunOverviewAttentionKind::Failed => "失败",
                    RunOverviewAttentionKind::Takeover => "等待用户接手",
                    _ => continue,
                };
                push(format!("{} {label}：{}", item.agent, item.title));
            }
            for agent in &project.overview.running {
                let doing = agent
                    .action
                    .as_deref()
                    .or(agent.task.as_deref())
                    .unwrap_or("运行中");
                push(format!("{} 运行中：{doing}", agent.name));
            }
        }
    }
    entries
}

pub(super) fn report_overview(workspaces: &[IslandWorkspace]) -> Option<String> {
    let entries = overview_entries(workspaces);
    (!entries.is_empty()).then(|| bounded_overview(&entries, OVERVIEW_CHARS))
}
