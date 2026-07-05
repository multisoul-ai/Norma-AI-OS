//! Background voice planning worker.

use crate::app::voice::log::{log_voice_planner_failed, log_voice_planning_started};
use crate::app::voice::refine::refine_speech_transcript;
use crate::settings::voice::VoiceSettingsValues;
use voice::{
    compile_voice_plan, CompiledVoicePlan, OllamaVoicePlanner, RiskLevel, VoicePlan,
    VoicePlanDecision, VoicePlanner, VoicePlanningRequest,
};

#[derive(Clone, Debug)]
pub(crate) struct VoicePlanningJob {
    pub(super) raw_transcript: String,
    pub(super) request: VoicePlanningRequest,
    pub(super) settings: VoiceSettingsValues,
}

#[derive(Clone, Debug)]
pub(crate) struct VoicePlanningResult {
    pub(super) raw_transcript: String,
    pub(super) refined_transcript: Option<String>,
    pub(super) transcript: String,
    pub(super) outcome: VoicePlanningOutcome,
}

#[derive(Clone, Debug)]
pub(super) enum VoicePlanningOutcome {
    ProviderError {
        error: String,
    },
    Rejected {
        planner_json: String,
        confidence: f64,
        risk: RiskLevel,
        error: String,
    },
    TranscriptOnly {
        planner_json: String,
        confidence: f64,
        risk: RiskLevel,
        reason: String,
    },
    ConfirmationRequired {
        planner_json: String,
        confidence: f64,
        risk: RiskLevel,
        compiled: CompiledVoicePlan,
    },
    Execute {
        planner_json: String,
        confidence: f64,
        risk: RiskLevel,
        compiled: CompiledVoicePlan,
    },
}

pub(super) fn run_voice_planning_job(mut job: VoicePlanningJob) -> VoicePlanningResult {
    let raw_transcript = job.raw_transcript;
    let (transcript, refined_transcript) = refine_speech_transcript(&raw_transcript, &job.settings);
    job.request.transcript = transcript.clone();
    let planner = OllamaVoicePlanner::new(
        job.settings.ollama_endpoint().to_string(),
        job.settings.planner_model().to_string(),
    );
    log_voice_planning_started(&transcript, &planner, &job.request);

    let outcome = match planner.plan(&job.request) {
        Ok(plan) => compile_voice_planning_outcome(&job.request, plan),
        Err(error) => {
            log_voice_planner_failed(&error, &transcript);
            VoicePlanningOutcome::ProviderError {
                error: error.to_string(),
            }
        }
    };

    VoicePlanningResult {
        raw_transcript,
        refined_transcript,
        transcript,
        outcome,
    }
}

fn compile_voice_planning_outcome(
    request: &VoicePlanningRequest,
    plan: VoicePlan,
) -> VoicePlanningOutcome {
    let planner_json = serde_json::to_string(&plan).unwrap_or_else(|_| "{}".to_string());
    let confidence = f64::from(plan.confidence);
    let risk = plan.risk;

    match compile_voice_plan(request, plan) {
        Ok(VoicePlanDecision::TranscriptOnly { reason }) => VoicePlanningOutcome::TranscriptOnly {
            planner_json,
            confidence,
            risk,
            reason,
        },
        Ok(VoicePlanDecision::Execute(compiled)) if compiled.requires_confirmation => {
            VoicePlanningOutcome::ConfirmationRequired {
                planner_json,
                confidence,
                risk,
                compiled,
            }
        }
        Ok(VoicePlanDecision::Execute(compiled)) => VoicePlanningOutcome::Execute {
            planner_json,
            confidence,
            risk,
            compiled,
        },
        Err(error) => VoicePlanningOutcome::Rejected {
            planner_json,
            confidence,
            risk,
            error: error.to_string(),
        },
    }
}
