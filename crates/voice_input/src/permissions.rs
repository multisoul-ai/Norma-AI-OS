use crate::VoiceInputError;

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

/// Debug E2E only: with an audio fixture configured the real microphone is never opened,
/// so there is nothing to authorise and no TCC prompt may be raised.
#[cfg(debug_assertions)]
pub(crate) fn fixture_replaces_microphone() -> bool {
    crate::fixture::fixture_path().is_some()
}

#[cfg(not(debug_assertions))]
pub(crate) fn fixture_replaces_microphone() -> bool {
    false
}

#[cfg(target_os = "macos")]
pub fn microphone_authorization_status() -> Result<MicrophoneAuthorization, VoiceInputError> {
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
    if fixture_replaces_microphone() {
        return Ok(MicrophoneAuthorization::Authorized);
    }
    let media_type = unsafe { AVMediaTypeAudio }
        .ok_or_else(|| VoiceInputError::Audio("audio media type unavailable".to_string()))?;
    microphone_authorization_from_raw(unsafe {
        AVCaptureDevice::authorizationStatusForMediaType(media_type).0 as i64
    })
    .ok_or_else(|| VoiceInputError::Audio("unknown microphone authorization status".to_string()))
}

#[cfg(target_os = "macos")]
pub fn ensure_microphone_authorized() -> Result<(), VoiceInputError> {
    use objc2_av_foundation::AVMediaTypeAudio;
    if fixture_replaces_microphone() {
        return Ok(());
    }
    let media_type = unsafe { AVMediaTypeAudio }
        .ok_or_else(|| VoiceInputError::Audio("audio media type unavailable".to_string()))?;
    let status = microphone_authorization_status()?;
    match status {
        MicrophoneAuthorization::Authorized => Ok(()),
        MicrophoneAuthorization::Denied => Err(VoiceInputError::MicrophoneDenied),
        MicrophoneAuthorization::Restricted => Err(VoiceInputError::MicrophoneRestricted),
        MicrophoneAuthorization::NotDetermined => request_microphone_authorization(media_type),
    }
}

#[cfg(target_os = "macos")]
fn request_microphone_authorization(
    media_type: &'static objc2_av_foundation::AVMediaType,
) -> Result<(), VoiceInputError> {
    use block2::RcBlock;
    use objc2_av_foundation::AVCaptureDevice;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let (tx, rx) = mpsc::channel::<bool>();
    let block = RcBlock::new(move |granted: objc2::runtime::Bool| {
        let _ = tx.send(granted.as_bool());
    });
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &block) };

    // The completion handler fires on an arbitrary Apple dispatch queue — NOT the main
    // CFRunLoop. Plain sleep is sufficient; pumping the main CFRunLoop causes GPUI
    // re-entrancy ("RefCell already borrowed") because GPUI holds a mutable app borrow
    // during event handling.
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match rx.try_recv() {
            Ok(true) => return Ok(()),
            Ok(false) => return Err(VoiceInputError::MicrophoneDenied),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(VoiceInputError::Audio(
                    "mic auth callback closed".to_string(),
                ))
            }
            Err(mpsc::TryRecvError::Empty) if Instant::now() >= deadline => {
                return Err(VoiceInputError::Audio(
                    "microphone authorization timed out".to_string(),
                ))
            }
            Err(mpsc::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn ensure_microphone_authorized() -> Result<(), VoiceInputError> {
    Err(VoiceInputError::Audio(
        "microphone not supported on this platform".to_string(),
    ))
}

#[cfg(not(target_os = "macos"))]
pub fn microphone_authorization_status() -> Result<MicrophoneAuthorization, VoiceInputError> {
    Err(VoiceInputError::Audio(
        "microphone not supported on this platform".to_string(),
    ))
}

#[cfg(test)]
#[path = "../tests/unit/permissions_tests.rs"]
mod tests;
