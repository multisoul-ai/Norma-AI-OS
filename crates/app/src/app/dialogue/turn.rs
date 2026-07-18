//! Voice turn orchestration: Mock LLM chat, inline tools, template reports.

use super::artifacts::{TemplateReportItem, VoiceTurnArtifacts};
#[cfg(test)]
use super::card_tools::AppCardToolExecutor;
use super::card_tools::{template_report_for_result, CardCommandRunner};
use dialogue::{
    decide_template_report, run_voice_turn, DialogueError, DialoguePhase, ReportPlayDecision,
    ToolExecutor,
};
use llm::{LlmClient, MockLlmClient, MockStep, OllamaLlmClient};
use norma::protocol::{CardCommand, CardCommandResult};
use std::sync::mpsc;
use std::sync::Mutex;

pub(crate) const DIALOGUE_SYSTEM_PROMPT: &str = "你是 Norma 语音助手。请用一到三句简短中文直接回答用户问题，给出与问题相关的实质信息，不能只回复“好的”或“收到”。需要操作画布时调用工具，工具完成后说明具体结果。回复将由语音合成播报，不要使用表情符号、Markdown、列表或代码块。";
pub(crate) const MAX_TOOL_ROUNDS: usize = 5;

pub(crate) struct VoiceTurnJob {
    transcript: String,
    llm: VoiceTurnLlm,
}

pub(crate) enum VoiceTurnLlm {
    Mock(Vec<MockStep>),
    Ollama { endpoint: String, model: String },
}

pub(crate) struct VoiceToolRequest {
    command: CardCommand,
    result_tx: mpsc::Sender<Result<CardCommandResult, String>>,
}

impl VoiceToolRequest {
    pub(crate) fn command(&self) -> &CardCommand {
        &self.command
    }

    pub(crate) fn complete(self, result: Result<CardCommandResult, String>) {
        let _ = self.result_tx.send(result);
    }
}

struct ChannelToolExecutor {
    request_tx: async_channel::Sender<VoiceToolRequest>,
}

impl ToolExecutor for ChannelToolExecutor {
    fn execute(&mut self, command: CardCommand) -> Result<CardCommandResult, DialogueError> {
        let (result_tx, result_rx) = mpsc::channel();
        self.request_tx
            .send_blocking(VoiceToolRequest { command, result_tx })
            .map_err(|_| DialogueError::Message("voice tool bridge closed".to_string()))?;
        result_rx
            .recv()
            .map_err(|_| DialogueError::Message("voice tool result bridge closed".to_string()))?
            .map_err(DialogueError::Message)
    }
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
        play_reply: true,
        executed_commands: outcome.executed_commands,
        template_reports,
        queued_template_reports,
    })
}

#[cfg(test)]
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

pub(crate) fn run_voice_turn_job(
    job: VoiceTurnJob,
    request_tx: async_channel::Sender<VoiceToolRequest>,
) -> Result<VoiceTurnArtifacts, DialogueError> {
    match job.llm {
        VoiceTurnLlm::Mock(script) => {
            let mut executor = ChannelToolExecutor { request_tx };
            let mut llm = MockLlmClient::script(script);
            run_dialogue_turn(
                &mut llm,
                &mut executor,
                &job.transcript,
                DialoguePhase::Thinking,
                false,
            )
        }
        VoiceTurnLlm::Ollama { endpoint, model } => {
            let mut executor = ChannelToolExecutor { request_tx };
            let mut llm = OllamaLlmClient::new(endpoint, model);
            run_dialogue_turn(
                &mut llm,
                &mut executor,
                &job.transcript,
                DialoguePhase::Thinking,
                false,
            )
        }
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
