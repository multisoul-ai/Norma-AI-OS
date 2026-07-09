//! Voice turn orchestration: Mock LLM chat, inline tools, template reports.

use super::card_tools::{
    template_report_for_result, AppCardToolExecutor, CanvasInlineCardRunner, CardCommandRunner,
};
use crate::app::MultisoulApp;
use dialogue::{
    decide_template_report, run_voice_turn, DialogueError, DialoguePhase, ReportPlayDecision,
    ToolExecutor,
};
use gpui::Window;
use llm::{LlmClient, MockLlmClient, MockStep};
use norma::protocol::CardCommand;
use std::sync::Mutex;

pub(crate) const DIALOGUE_SYSTEM_PROMPT: &str = "你是 Norma 语音助手。";
pub(crate) const MAX_TOOL_ROUNDS: usize = 5;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct TemplateReportItem {
    pub(crate) text: String,
    pub(crate) decision: ReportPlayDecision,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct VoiceTurnArtifacts {
    pub(crate) reply_text: String,
    pub(crate) executed_commands: Vec<CardCommand>,
    pub(crate) template_reports: Vec<TemplateReportItem>,
    pub(crate) queued_template_reports: Vec<String>,
}

pub(crate) fn default_echo_mock_script(transcript: &str) -> Vec<MockStep> {
    vec![MockStep::Text(format!("收到：{transcript}"))]
}

pub(crate) fn build_template_reports(
    command_results: &[norma::protocol::CardCommandResult],
    phase: DialoguePhase,
    user_speech_active: bool,
) -> (Vec<TemplateReportItem>, Vec<String>) {
    let mut template_reports = Vec::new();
    let mut queued_template_reports = Vec::new();

    for result in command_results {
        let Some(text) = template_report_for_result(result) else {
            continue;
        };
        let decision = decide_template_report(phase, user_speech_active);
        match decision {
            ReportPlayDecision::PlayNow => {}
            ReportPlayDecision::Queue | ReportPlayDecision::DeferWhileUserSpeaking => {
                queued_template_reports.push(text.clone());
            }
        }
        template_reports.push(TemplateReportItem { text, decision });
    }

    (template_reports, queued_template_reports)
}

pub(crate) fn run_dialogue_turn<L: LlmClient, E: ToolExecutor>(
    llm: &mut L,
    executor: &mut E,
    transcript: &str,
    phase: DialoguePhase,
    user_speech_active: bool,
) -> Result<VoiceTurnArtifacts, DialogueError> {
    let outcome = run_voice_turn(
        llm,
        executor,
        transcript,
        DIALOGUE_SYSTEM_PROMPT,
        MAX_TOOL_ROUNDS,
    )?;
    let (template_reports, queued_template_reports) =
        build_template_reports(&outcome.command_results, phase, user_speech_active);

    Ok(VoiceTurnArtifacts {
        reply_text: outcome.reply_text,
        executed_commands: outcome.executed_commands,
        template_reports,
        queued_template_reports,
    })
}

pub(crate) fn run_dialogue_turn_with_mock(
    transcript: &str,
    mock_script: Vec<MockStep>,
    runner: &mut dyn CardCommandRunner,
    phase: DialoguePhase,
    user_speech_active: bool,
) -> Result<VoiceTurnArtifacts, DialogueError> {
    let mut llm = MockLlmClient::script(mock_script);
    let mut executor = AppCardToolExecutor::new(runner);
    run_dialogue_turn(
        &mut llm,
        &mut executor,
        transcript,
        phase,
        user_speech_active,
    )
}

impl MultisoulApp {
    pub(crate) fn run_dialogue_voice_turn(
        &mut self,
        window: &mut Window,
        transcript: &str,
        mock_script: Vec<MockStep>,
    ) -> Result<VoiceTurnArtifacts, DialogueError> {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        let mut runner = CanvasInlineCardRunner {
            runtime: &mut self.runtime,
            motion: &mut self.motion,
            master_layout: &mut self.master_layout,
            area,
            zoom,
            executions: Vec::new(),
        };
        let outcome = run_dialogue_turn_with_mock(
            transcript,
            mock_script,
            &mut runner,
            DialoguePhase::Thinking,
            false,
        )?;
        for execution in runner.take_executions() {
            self.apply_voice_execution_effects(execution);
        }
        Ok(outcome)
    }

    pub(crate) fn run_dialogue_turn_after_final(
        &mut self,
        window: &mut Window,
    ) -> Result<VoiceTurnArtifacts, DialogueError> {
        let transcript = self.dialogue.pending_transcript_or_err()?.to_string();
        let mock_script = mock_script_for_transcript(&transcript);

        #[cfg(test)]
        if let Some(runner) =
            crate::app::dialogue::DialogueControlState::test_turn_runner_for_tests()
        {
            let artifacts = runner(&transcript, mock_script)?;
            self.dialogue.store_turn_artifacts(&artifacts);
            return Ok(artifacts);
        }

        let artifacts = self.run_dialogue_voice_turn(window, &transcript, mock_script)?;
        self.dialogue.store_turn_artifacts(&artifacts);
        Ok(artifacts)
    }
}

#[cfg(test)]
pub(crate) fn run_dialogue_turn_with_recording_runner(
    transcript: &str,
    mock_script: Vec<MockStep>,
) -> Result<VoiceTurnArtifacts, DialogueError> {
    use super::card_tools::RecordingCardCommandRunner;
    use norma::protocol::CardCommandResult;

    let mut runner = RecordingCardCommandRunner::new(|command| match command {
        CardCommand::Create { .. } => Ok(CardCommandResult::Created { node_id: 7 }),
        CardCommand::SendText { node_id, .. } => Ok(CardCommandResult::Sent { node_id }),
        CardCommand::Focus { node_id } => Ok(CardCommandResult::Focused { node_id }),
        other => Err(format!("RecordingCardCommandRunner 未实现: {other:?}")),
    });
    run_dialogue_turn_with_mock(
        transcript,
        mock_script,
        &mut runner,
        DialoguePhase::Thinking,
        false,
    )
}

type MockScriptFactory = fn(&str) -> Vec<MockStep>;

static MOCK_SCRIPT_FACTORY: Mutex<Option<MockScriptFactory>> = Mutex::new(None);

pub(crate) fn mock_script_for_transcript(transcript: &str) -> Vec<MockStep> {
    if let Some(factory) = *MOCK_SCRIPT_FACTORY
        .lock()
        .expect("mock script factory lock should not be poisoned")
    {
        return factory(transcript);
    }
    default_echo_mock_script(transcript)
}

#[cfg(test)]
pub(crate) fn set_mock_script_factory_for_tests(factory: Option<MockScriptFactory>) {
    *MOCK_SCRIPT_FACTORY
        .lock()
        .expect("mock script factory lock should not be poisoned") = factory;
}

#[cfg(test)]
#[path = "../../../tests/unit/app_dialogue_turn_tests.rs"]
mod app_dialogue_turn_tests;
