use crate::session::DialoguePhase;

#[cfg(test)]
#[path = "../tests/unit/report_queue_tests.rs"]
mod report_queue_tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportPlayDecision {
    PlayNow,
    Queue,
    DeferWhileUserSpeaking,
}

pub fn decide_template_report(
    phase: DialoguePhase,
    user_speech_active: bool,
) -> ReportPlayDecision {
    if user_speech_active {
        return ReportPlayDecision::DeferWhileUserSpeaking;
    }
    match phase {
        DialoguePhase::Speaking => ReportPlayDecision::Queue,
        DialoguePhase::Listening | DialoguePhase::Idle => ReportPlayDecision::PlayNow,
        DialoguePhase::Thinking => ReportPlayDecision::Queue,
    }
}
