// Real implementation in Task 3.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MicrophoneAuthorization {
    NotDetermined,
    Denied,
    Restricted,
    Authorized,
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
