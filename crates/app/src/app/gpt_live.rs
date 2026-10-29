//! Desktop Live call. Credentials, hardware and UI stay outside the Live protocol crate.
mod backend;
mod bridge;
mod capture;
mod context;
mod dispatch;
mod durable;
mod input;
mod native_report;
mod output;
mod prompt;
mod question;
mod reconciliation;
mod report;
mod session;
mod startup;
mod state;
mod tasks;
use crate::{
    app::MultisoulApp,
    settings::voice::{DesktopVoiceMode, VoiceEntryState},
};
use gpui::{Context, Window};
use session::{CallEnd, CallEvent};
pub(crate) use state::{GptLiveVoiceState, LiveCallView};

#[cfg(test)]
#[path = "../../tests/unit/gpt_live_brain_report_tests.rs"]
mod brain_report_tests;

impl MultisoulApp {
    pub(crate) fn select_desktop_voice_mode(
        &mut self,
        mode: DesktopVoiceMode,
        cx: &mut Context<Self>,
    ) {
        if self.voice_settings.values().desktop_mode() == mode {
            return;
        }
        self.stop_gpt_live_voice(cx);
        self.stop_doubao_voice(cx);
        self.stop_continuous_listening(cx);
        self.voice.stop_for_model_switch();
        self.tts_preview.cancel();
        self.cancel_agent_announcements();
        self.release_a1_runtime(cx);
        self.voice_settings.select_desktop_mode(mode);
        self.persist_voice_settings_in_background(cx);
        cx.notify();
    }

    pub(crate) fn stop_gpt_live_voice(&mut self, cx: &mut Context<Self>) {
        self.gpt_live.stop();
        cx.notify();
    }

    pub(crate) fn toggle_gpt_live_mute(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.gpt_live.request_mute() {
            tracing::debug!(%error,"Live microphone control unavailable");
        }
        cx.notify();
    }

    pub(crate) fn toggle_gpt_live_output(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.gpt_live.request_output() {
            tracing::debug!(%error, "Live playback control unavailable");
        }
        cx.notify();
    }

    pub(crate) fn toggle_gpt_live_voice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.gpt_live.is_active() {
            self.stop_gpt_live_voice(cx);
            return;
        }
        let input_enabled = self.voice_settings.values().gpt_live_interaction_mode()
            == crate::settings::voice::VoiceInteractionMode::ContinuousListening;
        self.start_gpt_live_voice(input_enabled, false, window, cx);
    }

    fn start_gpt_live_voice(
        &mut self,
        input_enabled: bool,
        from_shortcut: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.voice_settings.values().desktop_mode() != DesktopVoiceMode::GptLive
            || self.voice_settings.readiness() != VoiceEntryState::Ready
        {
            return;
        }
        let Some(account) = self
            .voice_settings
            .values()
            .gpt_live_profile()
            .credential_ref()
            .map(str::to_owned)
        else {
            return;
        };
        let Some(store) = self.voice_settings.credential_store() else {
            return;
        };
        self.stop_doubao_voice(cx);
        self.stop_continuous_listening(cx);
        self.voice.stop_for_model_switch();
        self.tts_preview.cancel();
        self.cancel_agent_announcements();
        let project = self
            .runtime
            .workspace_root()
            .map(|root| root.display().to_string());
        let shared_context = match self.shared_context_service() {
            Ok(service) => service,
            Err(error) => {
                self.gpt_live.fail(error.to_string());
                cx.notify();
                return;
            }
        };
        let context_scope = shared_context.scope().clone();
        let session_id = format!("live-call-{}", uuid::Uuid::new_v4());
        let (generation, cancellation) = self.gpt_live.begin();
        self.gpt_live.context_scope = Some(context_scope.clone());
        self.gpt_live.configure_input(
            self.voice_settings.values().gpt_live_interaction_mode()
                == crate::settings::voice::VoiceInteractionMode::SingleVoiceCommand,
            input_enabled,
            from_shortcut,
        );
        self.gpt_live.activation = Some(cx.observe_window_activation(window, |app, window, cx| {
            if !window.is_window_active() && !app.gpt_live.input_complete() {
                app.release_gpt_live_input(cx);
            }
        }));
        let (reports, report_input) = async_channel::bounded(8);
        self.gpt_live.reports = Some(reports);
        let config = startup::CallConfig {
            reports: report_input,
            shared_context,
            session_id: session_id.clone(),
            input_enabled,
            account,
            voice: self
                .voice_settings
                .values()
                .gpt_live_profile()
                .voice()
                .into(),
            store,
            input_device: self
                .voice_settings
                .values()
                .input_device_id()
                .map(str::to_owned),
            canvas: self.current_canvas_context_for_provider(),
            roster: self.agent_roster_for_provider(),
            playback: self
                .voice_window_inset_events
                .sink(super::voice_window_inset::TtsPlaybackSource::GptLive),
            vad_root: self.voice_settings.model_root().to_path_buf(),
        };
        let (commands, controls) = async_channel::bounded(8);
        let (task_ack, task_acks) = async_channel::bounded(128);
        let (task_enabled, task_enablement) = tokio::sync::watch::channel(
            self.voice_settings
                .values()
                .agent_task_announcements_enabled(),
        );
        let task_reporter = project.as_ref().and_then(|project| {
            self.persistence
                .project_store()
                .map(|store| tasks::TaskReporter {
                    database: self.database_paths.app_db_path().to_path_buf(),
                    project: project.clone(),
                    tasks: store.agent_dispatch_task_store(),
                    commands: commands.clone(),
                    acknowledgements: task_acks,
                    enabled: task_enablement,
                    cancellation: cancellation.child_token(),
                })
        });
        self.gpt_live.commands = Some(commands);
        let (events, updates) = async_channel::bounded(64);
        let (delegated, completions) = async_channel::bounded(16);
        let mut bridge = bridge::LiveBridge::new(session_id, cancellation.clone());
        self.gpt_live.bind_timeline(bridge.timeline.clone());
        let (finished, outcome) = tokio::sync::oneshot::channel();
        let runtime = match super::shared_tokio_runtime() {
            Ok(runtime) => runtime,
            Err(error) => {
                self.gpt_live.finish(
                    generation,
                    CallEnd {
                        usage: Default::default(),
                        error: Some(error),
                    },
                );
                cx.notify();
                return;
            }
        };
        let previous = self.gpt_live.task.take();
        self.gpt_live.task = Some(runtime.spawn(async move {
            if let Some(previous) = previous { let _ = previous.await; }
            let reporter = task_reporter.map(|reporter|tokio::spawn(reporter.run()));
            let call_cancellation = cancellation.clone();
            let result = if cancellation.is_cancelled() { CallEnd{usage:Default::default(),error:None} }
                else { startup::run_call(config,controls,events,cancellation).await };
            call_cancellation.cancel();
            if let Some(reporter) = reporter {
                if tokio::time::timeout(std::time::Duration::from_secs(1),reporter).await.is_err() {tracing::warn!("Live task reporter cleanup exceeded its deadline");}
            }
            tracing::info!(target:"norma_voice",generation,seconds=result.usage.seconds,finalized=result.usage.finalized,error=result.error.as_deref().unwrap_or(""),"GPT-Live call resources released");
            let _ = finished.send(result);
        }));
        cx.spawn(async move |this, cx| {
            let result = outcome.await.unwrap_or_else(|_| CallEnd {
                usage: Default::default(),
                error: Some("GPT-Live session worker ended unexpectedly".into()),
            });
            let _ = this.update(cx, |app, cx| {
                app.gpt_live.finish(generation, result);
                cx.notify();
            });
        })
        .detach();
        let mut reports = report::FinalReports::default();
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            let active = this
                .update_in(cx, |app, window, cx| {
                    if app.gpt_live.generation != generation || !app.gpt_live.is_active() {
                        return false;
                    }
                    if app
                        .runtime
                        .workspace_root()
                        .map(|root| root.display().to_string())
                        != project
                        || !app
                            .shared_context_service()
                            .is_ok_and(|service| service.scope() == &context_scope)
                        || app.voice_settings.values().desktop_mode() != DesktopVoiceMode::GptLive
                    {
                        app.stop_gpt_live_voice(cx);
                        return false;
                    }
                    task_enabled.send_if_modified(|enabled| {
                        let next = app
                            .voice_settings
                            .values()
                            .agent_task_announcements_enabled();
                        let changed = *enabled != next;
                        *enabled = next;
                        changed
                    });
                    while let Ok(event) = updates.try_recv() {
                        if let CallEvent::AppendAcknowledged(id) = &event {
                            if id.starts_with("live-task-") {
                                let _ = task_ack.try_send(id.clone());
                            }
                        }
                        if let CallEvent::Delegation(notice) = event {
                            if let Some(reason) = report::admission(
                                app.voice_settings.gpt_live_delegation_ready(),
                                app.gpt_live.vad_ready,
                            ) {
                                if let Err(error) = reports.push(notice.id, reason.into()) {
                                    app.gpt_live.fail(error);
                                    return false;
                                }
                            } else if bridge.delegation(notice).is_err() {
                                app.stop_gpt_live_voice(cx);
                                return false;
                            }
                        } else {
                            if let CallEvent::SpeechActivity(observation) = &event {
                                if observation
                                    .events
                                    .contains(&voice_input::VadEvent::SpeechStart)
                                {
                                    bridge.speech_started();
                                }
                            }
                            if let CallEvent::Transcript(part) = &event {
                                if bridge.transcript(part.clone()).is_err() {
                                    app.stop_gpt_live_voice(cx);
                                    return false;
                                }
                            }
                            app.gpt_live.apply(generation, event);
                        }
                        cx.notify();
                    }
                    while let Ok(completed) = completions.try_recv() {
                        app.gpt_live.planning = None;
                        let dispatch::DelegatedCompletion { work, report } = completed;
                        if let Some(content) = bridge.complete(&work, report) {
                            if let Err(error) = reports.push(work.snapshot.delegation_id, content) {
                                app.gpt_live.fail(error);
                                return false;
                            }
                        }
                    }
                    if let Some(commands) = &app.gpt_live.commands {
                        if let Err(error) = reports.flush(commands) {
                            app.gpt_live.fail(error);
                            return false;
                        }
                    }
                    if reports.is_empty() && app.gpt_live.input_complete() {
                        if let Some(work) = bridge.claim_next() {
                            if let Some(reason) = report::admission(
                                app.voice_settings.gpt_live_delegation_ready(),
                                app.gpt_live.vad_ready,
                            ) {
                                let _ = bridge.complete(&work, report::DelegatedReport::default());
                                if let Err(error) =
                                    reports.push(work.snapshot.delegation_id, reason.into())
                                {
                                    app.gpt_live.fail(error);
                                    return false;
                                }
                            } else {
                                let scope = super::agent_runtime::LiveAdapterScope {
                                    // Empty means personal/overview: read-only context only.
                                    project: project.clone().unwrap_or_default(),
                                    generation,
                                    timeline: bridge.timeline.clone(),
                                    snapshot: work.snapshot.clone(),
                                };
                                app.gpt_live.planning = Some(work.cancellation.clone());
                                dispatch::start(app, work, scope, delegated.clone(), window, cx);
                            }
                        }
                    }
                    true
                })
                .unwrap_or(false);
            if !active {
                break;
            }
        })
        .detach();
        cx.notify();
    }
}
