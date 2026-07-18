//! Voice turn reply scheduling and TTS playback after ASR finals.

#[cfg(test)]
use super::reply::{apply_reply_synthesis, create_tts_client};
#[cfg(test)]
use super::turn;
use super::{artifacts::VoiceTurnArtifacts, DialogueControlState};
use dialogue::DialoguePhase;
use gpui::Context;
#[cfg(test)]
use voice_sherpa::SpeechError;

#[cfg(test)]
type TestTurnRunner =
    fn(&str, Vec<llm::MockStep>) -> Result<VoiceTurnArtifacts, dialogue::DialogueError>;

#[cfg(test)]
static TEST_TURN_RUNNER: std::sync::Mutex<Option<TestTurnRunner>> = std::sync::Mutex::new(None);

impl DialogueControlState {
    pub(crate) fn has_pending_spoken_reply(&self) -> bool {
        self.pending_voice_turn
            || (self.session.phase() == DialoguePhase::Listening
                && !self.queued_template_reports.is_empty())
            || self.reply_task.is_some()
            || self.speech_task.is_some()
    }

    pub(crate) fn mark_voice_turn_pending(&mut self) {
        self.pending_voice_turn = true;
    }

    pub(crate) fn take_pending_voice_turn(&mut self) -> bool {
        let pending = self.pending_voice_turn;
        self.pending_voice_turn = false;
        pending
    }

    pub(crate) fn queue_agent_report(&mut self, report: String) {
        self.queued_template_reports.push(report);
    }

    pub(crate) fn begin_next_agent_report(&mut self) -> Option<String> {
        if self.session.phase() != DialoguePhase::Listening
            || self.reply_task.is_some()
            || self.speech_task.is_some()
            || self.queued_template_reports.is_empty()
        {
            return None;
        }
        self.session.on_report_pending().ok()?;
        self.queued_template_reports.first().cloned()
    }

    pub(crate) fn complete_current_agent_report(&mut self) {
        if !self.queued_template_reports.is_empty() {
            self.queued_template_reports.remove(0);
        }
    }

    pub(crate) fn begin_queued_agent_report(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) {
        let Some(report) = self.begin_next_agent_report() else {
            return;
        };
        self.complete_current_agent_report();
        self.begin_static_reply_stream(report, cx);
    }

    pub(crate) fn begin_voice_turn_reply(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<crate::app::MultisoulApp>,
    ) {
        if self.session.phase() != DialoguePhase::Thinking {
            return;
        }
        if self.reply_task.is_some() {
            return;
        }
        self.begin_reply_stream();
        let task = cx.spawn_in(window, async move |this, cx| {
            let response = match this.update_in(cx, |app, window, cx| {
                let transcript = app.dialogue.pending_transcript_or_err()?.to_string();
                app.submit_project_agent(agent::AgentCommand::UserTurn(transcript), window, cx)
                    .map_err(dialogue::DialogueError::Message)
            }) {
                Ok(Ok(response)) => response,
                Ok(Err(error)) => {
                    tracing::warn!(%error, "voice turn setup failed");
                    let _ = this.update(cx, |app, cx| {
                        app.dialogue.error_message = Some(format!("Turn error: {error}"));
                        app.dialogue.reply_task = None;
                        cx.notify();
                    });
                    return;
                }
                Err(_) => return,
            };
            let turn_result = response
                .recv()
                .await
                .map_err(|_| dialogue::DialogueError::Message("agent response closed".into()))
                .and_then(|result| result.map_err(dialogue::DialogueError::Message));
            match turn_result {
                Ok(events) => {
                    let has_live_dialogue = events
                        .iter()
                        .any(|event| matches!(event, agent::AgentEvent::DialogueTextDelta { .. }));
                    let artifacts = crate::app::agent_runtime::artifacts_from_agent_events(events);
                    let _ = this.update(cx, |app, cx| {
                        app.dialogue.store_turn_artifacts(&artifacts);
                        app.dialogue.reply_task = None;
                        if !has_live_dialogue {
                            app.dialogue
                                .begin_static_reply_stream(artifacts.reply_text.clone(), cx);
                        }
                        cx.notify();
                    });
                }
                Err(error) => {
                    tracing::warn!(%error, "voice turn failed");
                    let _ = this.update(cx, |app, cx| {
                        app.dialogue
                            .recover_reply_failure(format!("Turn error: {error}"));
                        cx.notify();
                    });
                }
            }
        });
        self.reply_task = Some(task);
    }

    pub(crate) fn store_turn_artifacts(&mut self, artifacts: &VoiceTurnArtifacts) {
        self.last_reply_text = Some(artifacts.reply_text.clone());
        self.queued_template_reports
            .extend(artifacts.queued_template_reports.clone());
    }

    #[cfg(test)]
    pub(crate) fn last_reply_text_for_tests(&self) -> Option<&str> {
        self.last_reply_text.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn queued_template_reports_for_tests(&self) -> &[String] {
        &self.queued_template_reports
    }

    #[cfg(test)]
    pub(crate) fn set_test_turn_runner_for_tests(runner: Option<TestTurnRunner>) {
        *TEST_TURN_RUNNER
            .lock()
            .expect("test turn runner lock should not be poisoned") = runner;
    }

    #[cfg(test)]
    pub(crate) fn test_turn_runner_for_tests() -> Option<TestTurnRunner> {
        *TEST_TURN_RUNNER
            .lock()
            .expect("test turn runner lock should not be poisoned")
    }

    #[cfg(test)]
    pub(crate) fn run_voice_turn_for_tests(
        &mut self,
    ) -> Result<VoiceTurnArtifacts, dialogue::DialogueError> {
        let transcript = self
            .session
            .pending_transcript()
            .ok_or_else(|| dialogue::DialogueError::Message("missing pending_transcript".into()))?
            .to_string();
        let mock_script = turn::mock_script_for_transcript(&transcript);
        if let Some(runner) = Self::test_turn_runner_for_tests() {
            let artifacts = runner(&transcript, mock_script)?;
            self.store_turn_artifacts(&artifacts);
            return Ok(artifacts);
        }
        let artifacts = turn::run_dialogue_turn_with_recording_runner(&transcript, mock_script)?;
        self.store_turn_artifacts(&artifacts);
        Ok(artifacts)
    }

    #[cfg(test)]
    pub(crate) fn on_reply_ready_for_tests(&mut self) -> Result<(), SpeechError> {
        let reply_text = self
            .last_reply_text
            .clone()
            .or_else(|| {
                self.session
                    .pending_transcript()
                    .map(turn::default_echo_mock_script)
                    .and_then(|steps| {
                        steps.into_iter().next().and_then(|step| match step {
                            llm::MockStep::Text(text) => Some(text),
                            _ => None,
                        })
                    })
            })
            .ok_or_else(|| SpeechError::Recognizer("missing reply text".to_string()))?;
        if self.tts_client.is_none() {
            self.tts_client = Some(create_tts_client()?);
        }
        let tts = self
            .tts_client
            .as_mut()
            .expect("tts client should exist after lazy init");
        apply_reply_synthesis(
            &mut self.session,
            tts.as_mut(),
            &mut self.reply_playback,
            &reply_text,
        )
        .map(|_| ())
    }
}
