#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceRequirement {
    Microphone,
    SpeechRecognition,
    AgentProvider,
    SpokenReplies,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceRequirementState {
    Missing,
    Installing { completed: u64, total: u64 },
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VoiceReadinessInputs {
    pub(crate) microphone: VoiceRequirementState,
    pub(crate) speech_recognition: VoiceRequirementState,
    pub(crate) agent_provider: VoiceRequirementState,
    pub(crate) spoken_replies: VoiceRequirementState,
    pub(crate) model_install_progress: Option<(u64, u64)>,
    pub(crate) setup_was_completed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceEntryState {
    SetUp { first_incomplete: VoiceRequirement },
    SettingUp { percent: u8 },
    Ready,
    Unavailable { first_incomplete: VoiceRequirement },
}

pub(crate) fn evaluate_voice_readiness(inputs: VoiceReadinessInputs) -> VoiceEntryState {
    let requirements = [
        (VoiceRequirement::Microphone, inputs.microphone),
        (
            VoiceRequirement::SpeechRecognition,
            inputs.speech_recognition,
        ),
        (VoiceRequirement::AgentProvider, inputs.agent_provider),
        (VoiceRequirement::SpokenReplies, inputs.spoken_replies),
    ];

    if requirements
        .iter()
        .all(|(_, state)| *state == VoiceRequirementState::Ready)
    {
        return VoiceEntryState::Ready;
    }

    if let Some((completed, total)) = inputs.model_install_progress {
        let completed = u128::from(completed);
        let total = u128::from(total);
        let percent = if total == 0 {
            0
        } else {
            ((completed.min(total) * 100) / total) as u8
        };
        return VoiceEntryState::SettingUp { percent };
    }

    let first_incomplete = requirements
        .iter()
        .find_map(|(requirement, state)| {
            (*state != VoiceRequirementState::Ready).then_some(*requirement)
        })
        .expect("non-ready Voice requirements must include an incomplete item");
    if inputs.setup_was_completed {
        VoiceEntryState::Unavailable { first_incomplete }
    } else {
        VoiceEntryState::SetUp { first_incomplete }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/voice_readiness_tests.rs"]
mod voice_readiness_tests;
