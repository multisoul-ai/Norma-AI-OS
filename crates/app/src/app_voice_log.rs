//! Tracing helpers for voice planning and execution.

use std::fmt::Display;
use voice::{OllamaVoicePlanner, VoicePlanningRequest};

pub(crate) fn log_voice_planning_started(
    transcript: &str,
    planner: &OllamaVoicePlanner,
    request: &VoicePlanningRequest,
) {
    tracing::info!(
        target: "app::voice",
        transcript,
        endpoint = %planner.endpoint(),
        model = %planner.model(),
        cards = request.cards.len(),
        selected = request.selected_node_ids.len(),
        focused = ?request.focused_node_id,
        max_steps = request.max_steps,
        "voice planning started"
    );
}

pub(crate) fn log_voice_planner_failed(error: impl Display, transcript: &str) {
    tracing::warn!(
        target: "app::voice",
        transcript,
        error = %error,
        "voice planner provider failed"
    );
}

pub(crate) fn log_voice_planner_rejected(
    transcript: &str,
    planner_json: &str,
    error: impl Display,
) {
    tracing::warn!(
        target: "app::voice",
        transcript,
        planner_json,
        error = %error,
        "voice planner output rejected"
    );
}

pub(crate) fn log_voice_transcript_only(
    transcript: &str,
    reason: &str,
    confidence: f64,
    risk: &'static str,
) {
    tracing::info!(
        target: "app::voice",
        transcript,
        reason,
        confidence,
        risk,
        "voice plan transcript only"
    );
}

pub(crate) fn log_voice_confirmation_required(
    transcript: &str,
    commands: usize,
    reason: &str,
    confidence: f64,
    risk: &'static str,
) {
    tracing::info!(
        target: "app::voice",
        transcript,
        commands,
        reason,
        confidence,
        risk,
        "voice plan requires confirmation"
    );
}

pub(crate) fn log_voice_executing(
    transcript: &str,
    commands: usize,
    confidence: f64,
    risk: &'static str,
) {
    tracing::info!(
        target: "app::voice",
        transcript,
        commands,
        confidence,
        risk,
        "voice plan executing"
    );
}

pub(crate) fn log_voice_executed(transcript: &str, message: &str) {
    tracing::info!(
        target: "app::voice",
        transcript,
        message,
        "voice plan executed"
    );
}

pub(crate) fn log_voice_runtime_failed(transcript: &str, error: &str) {
    tracing::warn!(
        target: "app::voice",
        transcript,
        error,
        "voice plan runtime execution failed"
    );
}
