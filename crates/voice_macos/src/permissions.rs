#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpeechAuthorization {
    NotDetermined,
    Denied,
    Restricted,
    Authorized,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MicrophoneAuthorization {
    NotDetermined,
    Denied,
    Restricted,
    Authorized,
}

pub fn speech_authorization_from_raw(raw: i64) -> Option<SpeechAuthorization> {
    match raw {
        0 => Some(SpeechAuthorization::NotDetermined),
        1 => Some(SpeechAuthorization::Denied),
        2 => Some(SpeechAuthorization::Restricted),
        3 => Some(SpeechAuthorization::Authorized),
        _ => None,
    }
}

pub fn microphone_authorization_from_raw(raw: i64) -> Option<MicrophoneAuthorization> {
    match raw {
        0 => Some(MicrophoneAuthorization::NotDetermined),
        1 => Some(MicrophoneAuthorization::Restricted),
        2 => Some(MicrophoneAuthorization::Denied),
        3 => Some(MicrophoneAuthorization::Authorized),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/unit/permissions_tests.rs"]
mod tests;
