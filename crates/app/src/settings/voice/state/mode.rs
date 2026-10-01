#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum DesktopVoiceMode {
    #[default]
    Legacy,
    Doubao,
}

impl DesktopVoiceMode {
    pub(crate) fn from_stored(value: &str) -> Option<Self> {
        match value.trim() {
            "legacy" => Some(Self::Legacy),
            "doubao" => Some(Self::Doubao),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Doubao => "doubao",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum VoiceInteractionMode {
    #[default]
    SingleVoiceCommand,
    ContinuousListening,
}

impl VoiceInteractionMode {
    pub(crate) fn from_stored(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dictation" | "single_voice_command" => Some(Self::SingleVoiceCommand),
            "continuous_listening" => Some(Self::ContinuousListening),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::SingleVoiceCommand => "single_voice_command",
            Self::ContinuousListening => "continuous_listening",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum DictationTrigger {
    #[default]
    Hold,
    Toggle,
}

impl DictationTrigger {
    pub(crate) fn from_stored(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "hold" => Some(Self::Hold),
            "toggle" => Some(Self::Toggle),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::Toggle => "toggle",
        }
    }
}
