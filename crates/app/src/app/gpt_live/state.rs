use super::session::{CallCommand, CallEnd, CallEvent};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_caption_tests.rs"]
mod caption_tests;

#[derive(Clone, Debug)]
pub(crate) struct LiveCallView {
    pub active: bool,
    pub muted: bool,
    pub can_mute: bool,
    pub output_blocked: bool,
    pub can_output: bool,
    pub tools_error: Option<String>,
    pub reconciliation: Option<String>,
    pub reconciling: bool,
    pub status: String,
    pub usage: Option<live_voice::LiveUsage>,
    pub usage_pending: bool,
}

#[derive(Default)]
enum CallStatus {
    #[default]
    Idle,
    Connecting,
    Connected,
    Failed(String),
}

#[derive(Default)]
pub(crate) struct GptLiveVoiceState {
    pub(super) context_scope: Option<agent::context::ContextScope>,
    pub(super) report_cancellation: CancellationToken,
    pub(super) vad_ready: bool,
    pub(super) reconciliation: Option<(String, String)>,
    pub(super) reconciliation_generation: u64,
    pub(super) reconciling: bool,
    status: CallStatus,
    pub(super) generation: u64,
    last_call_generation: Option<u64>,
    cancellation: Option<CancellationToken>,
    pub(super) commands: Option<async_channel::Sender<CallCommand>>,
    pub(super) reports: Option<async_channel::Sender<super::native_report::NativeReport>>,
    pub(super) task: Option<tokio::task::JoinHandle<()>>,
    pub(super) activation: Option<gpui::Subscription>,
    pub(super) planning: Option<CancellationToken>,
    pub(super) recording_from_shortcut: bool,
    hold_mode: bool,
    input_enabled: bool,
    release_pending: bool,
    input_revision: u64,
    input_timeline: Option<std::sync::Arc<std::sync::Mutex<live_voice::LiveContext>>>,
    tools_error: Option<String>,
    muted: bool,
    mute_pending: bool,
    output_blocked: bool,
    output_pending: bool,
    usage: live_voice::LiveUsage,
    usage_reported: bool,
    user_caption: String,
    assistant_caption: String,
}

impl GptLiveVoiceState {
    pub(crate) fn begin(&mut self) -> (u64, CancellationToken) {
        self.stop();
        let cancellation = CancellationToken::new();
        self.cancellation = Some(cancellation.clone());
        self.last_call_generation = Some(self.generation);
        self.status = CallStatus::Connecting;
        self.user_caption.clear();
        self.assistant_caption.clear();
        self.usage = live_voice::LiveUsage::default();
        self.usage_reported = false;
        self.vad_ready = false;
        self.tools_error = None;
        self.configure_input(false, true, false);
        (self.generation, cancellation)
    }

    pub(crate) fn stop(&mut self) {
        self.cancel_brain_reports();
        self.context_scope = None;
        self.generation = self.generation.wrapping_add(1);
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
        self.commands = None;
        self.reports = None;
        self.activation = None;
        self.planning = None;
        self.recording_from_shortcut = false;
        self.input_enabled = false;
        self.release_pending = false;
        self.input_timeline = None;
        self.muted = false;
        self.mute_pending = false;
        self.output_blocked = false;
        self.output_pending = false;
        self.status = CallStatus::Idle;
    }

    pub(super) fn fail(&mut self, message: String) {
        self.stop();
        self.status = CallStatus::Failed(message);
    }

    pub(crate) fn view(&self) -> LiveCallView {
        LiveCallView {
            reconciliation: None,
            reconciling: self.reconciling,
            active: self.is_active(),
            muted: self.is_muted(),
            can_mute: self.can_mute(),
            output_blocked: self.output_blocked,
            can_output: self.is_active()
                && matches!(self.status, CallStatus::Connected)
                && !self.output_pending,
            tools_error: self.tools_error.clone(),
            status: match &self.status {
                CallStatus::Failed(error) => format!("{}：{error}", self.label()),
                _ => self.label().into(),
            },
            usage: self.usage_reported.then(|| self.usage()),
            usage_pending: self.last_call_generation.is_some() && !self.usage_reported,
        }
    }

    pub(crate) fn view_for_project(&self, project: Option<&std::path::Path>) -> LiveCallView {
        let mut view = self.view();
        let project = project
            .map(|root| root.display().to_string())
            .unwrap_or_default();
        view.reconciliation = self
            .reconciliation
            .as_ref()
            .filter(|(root, _)| root == &project)
            .map(|(_, text)| text.clone());
        view
    }

    pub(crate) fn is_active(&self) -> bool {
        self.cancellation.is_some()
    }
    pub(super) fn can_report(&self) -> bool {
        self.is_active()
            && matches!(self.status, CallStatus::Connected)
            && !self.output_blocked
            && !self.output_pending
    }

    pub(crate) fn cancel_brain_reports(&mut self) {
        self.report_cancellation.cancel();
        self.report_cancellation = CancellationToken::new();
    }
    pub(crate) fn matches_generation(&self, generation: u64) -> bool {
        self.generation == generation && self.is_active()
    }
    pub(crate) fn is_muted(&self) -> bool {
        self.muted
    }
    pub(crate) fn usage(&self) -> live_voice::LiveUsage {
        self.usage
    }
    pub(crate) fn can_mute(&self) -> bool {
        self.is_active() && matches!(self.status, CallStatus::Connected) && !self.mute_pending
    }
    pub(super) fn request_mute(&mut self) -> Result<(), String> {
        if !self.can_mute() {
            return Err("Live microphone control is not ready".into());
        }
        self.commands
            .as_ref()
            .ok_or("Live controls closed")?
            .try_send(CallCommand::SetMuted(!self.muted))
            .map_err(|_| "Live controls are busy or closed")?;
        self.mute_pending = true;
        Ok(())
    }

    pub(super) fn request_output(&mut self) -> Result<(), String> {
        if !self.view().can_output {
            return Err("Live playback control is not ready".into());
        }
        self.commands
            .as_ref()
            .ok_or("Live controls closed")?
            .try_send(CallCommand::SetOutputBlocked(!self.output_blocked))
            .map_err(|_| "Live controls are busy or closed")?;
        self.output_pending = true;
        Ok(())
    }

    pub(super) fn configure_input(&mut self, hold_mode: bool, enabled: bool, from_shortcut: bool) {
        self.hold_mode = hold_mode;
        self.input_enabled = enabled;
        self.release_pending = false;
        self.recording_from_shortcut = from_shortcut;
    }

    pub(crate) fn input_complete(&self) -> bool {
        !self.hold_mode || (!self.input_enabled && !self.release_pending)
    }

    pub(crate) fn is_shortcut_input(&self) -> bool {
        self.recording_from_shortcut
    }

    pub(super) fn input_enabled(&self) -> bool {
        self.input_enabled
    }

    pub(super) fn bind_timeline(
        &mut self,
        timeline: std::sync::Arc<std::sync::Mutex<live_voice::LiveContext>>,
    ) {
        self.input_timeline = Some(timeline);
    }

    pub(super) fn request_input(
        &mut self,
        enabled: bool,
        from_shortcut: bool,
    ) -> Result<(), String> {
        if !self.is_active() || !self.hold_mode {
            return Ok(());
        }
        if !enabled && self.input_enabled && matches!(self.status, CallStatus::Connecting) {
            self.stop();
            return Ok(());
        }
        if enabled && !self.input_enabled {
            if let Some(timeline) = &self.input_timeline {
                timeline
                    .lock()
                    .map_err(|_| "Live input timeline unavailable")?
                    .suspend_user_input();
            }
        }
        let revision = self.input_revision.wrapping_add(1);
        self.commands
            .as_ref()
            .ok_or("Live controls closed")?
            .try_send(CallCommand::SetInputEnabled { enabled, revision })
            .map_err(|_| "Live input controls are busy or closed")?;
        self.input_revision = revision;
        self.input_enabled = enabled;
        self.release_pending = !enabled;
        self.recording_from_shortcut = enabled && from_shortcut;
        if enabled {
            if let Some(planning) = &self.planning {
                planning.cancel();
            }
        }
        Ok(())
    }

    pub(super) fn apply(&mut self, generation: u64, event: CallEvent) -> bool {
        if generation != self.generation || !self.is_active() {
            return false;
        }
        match event {
            CallEvent::InputReleased(revision) => {
                if revision == self.input_revision && !self.input_enabled {
                    self.release_pending = false;
                }
            }
            CallEvent::ToolsUnavailable(error) => self.tools_error = Some(error),
            CallEvent::VadReady(ready) => self.vad_ready = ready,
            CallEvent::OutputBlocked(blocked) => {
                self.output_blocked = blocked;
                self.output_pending = false;
            }
            CallEvent::Delegation(_)
            | CallEvent::SpeechActivity(_)
            | CallEvent::AppendAcknowledged(_) => return false,
            CallEvent::Usage(usage) => {
                self.usage = usage;
                self.usage_reported = true;
            }
            CallEvent::Connected => self.status = CallStatus::Connected,
            CallEvent::Muted(muted) => {
                self.muted = muted;
                self.mute_pending = false;
            }
            CallEvent::Transcript(part) => append_caption(
                match part.speaker {
                    live_voice::Speaker::User => &mut self.user_caption,
                    live_voice::Speaker::Assistant => &mut self.assistant_caption,
                },
                &part.delta,
            ),
        }
        true
    }

    pub(super) fn finish(&mut self, generation: u64, result: CallEnd) -> bool {
        if self.last_call_generation == Some(generation) {
            if result.usage.seconds >= self.usage.seconds {
                self.usage = result.usage;
            } else {
                // A worker failure may carry an unknown default; keep the last service snapshot.
                self.usage.finalized = false;
            }
            self.usage_reported |= result.usage.finalized || result.usage.seconds > 0.0;
        }
        if self.generation != generation {
            return false;
        }
        self.cancel_brain_reports();
        self.context_scope = None;
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
        self.commands = None;
        self.reports = None;
        self.mute_pending = false;
        self.activation = None;
        self.planning = None;
        self.recording_from_shortcut = false;
        self.input_enabled = false;
        self.release_pending = false;
        self.input_timeline = None;
        self.status = match result.error {
            Some(error) => CallStatus::Failed(error),
            None => CallStatus::Idle,
        };
        true
    }

    pub(crate) fn label(&self) -> &'static str {
        ui::i18n::text(match self.status {
            CallStatus::Idle => "voice.live_name",
            CallStatus::Connecting => "voice.connecting",
            CallStatus::Connected if self.muted => "voice.live_muted",
            CallStatus::Connected if self.hold_mode && !self.input_enabled => {
                "voice.live_waiting_hold"
            }
            CallStatus::Connected => "voice.live_in_call",
            CallStatus::Failed(_) => "voice.live_error",
        })
    }
    pub(crate) fn visual(&self) -> crate::app::voice::control::VoiceVisual {
        use crate::app::voice::control::VoiceVisual;
        match self.status {
            CallStatus::Idle => VoiceVisual::Idle,
            CallStatus::Connecting => VoiceVisual::Processing,
            CallStatus::Connected if self.muted => VoiceVisual::Idle,
            CallStatus::Connected if self.hold_mode && !self.input_enabled => VoiceVisual::Idle,
            CallStatus::Connected => VoiceVisual::Recording,
            CallStatus::Failed(_) => VoiceVisual::Error,
        }
    }
    pub(crate) fn has_caption(&self) -> bool {
        self.is_active() || matches!(self.status, CallStatus::Failed(_))
    }
    pub(crate) fn caption(&self) -> String {
        if let CallStatus::Failed(error) = &self.status {
            return format!("GPT-Live：{error}");
        }
        if self.user_caption.is_empty() && self.assistant_caption.is_empty() {
            return if self.muted
                || self.input_complete() && self.hold_mode
                || matches!(self.status, CallStatus::Connecting)
            {
                self.label()
            } else {
                ui::i18n::text("voice.live_listening")
            }
            .into();
        }
        format!(
            "{}：{}\nNorma：{}",
            ui::i18n::text("voice.live_you"),
            self.user_caption,
            self.assistant_caption
        )
    }
}

fn append_caption(text: &mut String, delta: &str) {
    text.push_str(delta);
    if text.len() > 8192 {
        let mut cut = text.len() - 8192;
        while !text.is_char_boundary(cut) {
            cut += 1;
        }
        text.drain(..cut);
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_call_state_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_hold_tests.rs"]
mod hold_tests;
