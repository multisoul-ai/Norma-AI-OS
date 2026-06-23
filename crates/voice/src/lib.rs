//! Voice domain layer: STT transcript planning, LLM schema validation, and card command compilation.

mod ollama;
mod ollama_repair;
mod plan;

pub use ollama::{OllamaVoicePlanner, VoicePlanner};
pub use plan::{
    compile_voice_plan, CardRef, CompiledVoicePlan, ExecutableCommand, PlannedCardKind,
    PlannedCommand, PlannedTarget, RiskLevel, Transcript, VoiceCardKind, VoicePlan,
    VoicePlanDecision, VoicePlanError, VoicePlanningRequest, MIN_EXECUTE_CONFIDENCE,
};
