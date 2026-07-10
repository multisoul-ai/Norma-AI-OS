//! Voice turn reply scheduling and TTS playback after ASR finals.

use super::{
    reply::{apply_reply_synthesis, create_tts_client, finish_reply_playback},
    turn::{self, VoiceTurnArtifacts},
    DialogueControlState,
};
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
    pub(crate) fn mark_voice_turn_pending(&mut self) {
        self.pending_voice_turn = true;
    }

    pub(crate) fn take_pending_voice_turn(&mut self) -> bool {
        let pending = self.pending_voice_turn;
        self.pending_voice_turn = false;
        pending
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
        let task = cx.spawn_in(window, async move |this, cx| {
            let turn_result = this.update_in(cx, |app, window, _cx| {
                app.run_dialogue_turn_after_final(window)
            });
            let Ok(turn_result) = turn_result else {
                return;
            };
            match turn_result {
                Ok(artifacts) => {
                    let synthesis_result = this.update(cx, |app, _cx| {
                        if app.dialogue.tts_client.is_none() {
                            app.dialogue.tts_client = Some(create_tts_client()?);
                        }
                        let tts = app
                            .dialogue
                            .tts_client
                            .as_mut()
                            .expect("tts client should exist after lazy init");
                        apply_reply_synthesis(
                            &mut app.dialogue.session,
                            tts.as_mut(),
                            &mut app.dialogue.reply_playback,
                            &artifacts.reply_text,
                        )
                    });
                    let Ok(synthesis_result) = synthesis_result else {
                        return;
                    };
                    match synthesis_result {
                        Ok(()) => {
                            let _ = this.update(cx, |app, cx| {
                                finish_reply_playback(&mut app.dialogue.session, true);
                                app.dialogue.reply_task = None;
                                cx.notify();
                            });
                        }
                        Err(error) => {
                            tracing::warn!(%error, "voice turn reply synthesis failed");
                            let _ = this.update(cx, |app, cx| {
                                app.dialogue.error_message = Some(format!("Speech error: {error}"));
                                app.dialogue.reply_task = None;
                                cx.notify();
                            });
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "voice turn failed");
                    let _ = this.update(cx, |app, cx| {
                        app.dialogue.error_message = Some(format!("Turn error: {error}"));
                        app.dialogue.reply_task = None;
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
                    .map(|text| turn::default_echo_mock_script(text))
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
    }
}
