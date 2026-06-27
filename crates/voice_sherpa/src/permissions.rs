use crate::SpeechError;

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

#[cfg(target_os = "macos")]
pub(crate) fn ensure_microphone_authorized() -> Result<(), SpeechError> {
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
    let media_type = unsafe { AVMediaTypeAudio }
        .ok_or_else(|| SpeechError::Audio("audio media type unavailable".to_string()))?;
    let status = microphone_authorization_from_raw(unsafe {
        AVCaptureDevice::authorizationStatusForMediaType(media_type).0 as i64
    })
    .ok_or_else(|| SpeechError::Audio("unknown microphone authorization status".to_string()))?;
    match status {
        MicrophoneAuthorization::Authorized => Ok(()),
        MicrophoneAuthorization::Denied => Err(SpeechError::MicrophoneDenied),
        MicrophoneAuthorization::Restricted => Err(SpeechError::MicrophoneRestricted),
        MicrophoneAuthorization::NotDetermined => request_microphone_authorization(media_type),
    }
}

#[cfg(target_os = "macos")]
fn request_microphone_authorization(
    media_type: &'static objc2_av_foundation::AVMediaType,
) -> Result<(), SpeechError> {
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
            Ok(false) => return Err(SpeechError::MicrophoneDenied),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(SpeechError::Audio("mic auth callback closed".to_string()))
            }
            Err(mpsc::TryRecvError::Empty) if Instant::now() >= deadline => {
                return Err(SpeechError::Audio(
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
pub(crate) fn ensure_microphone_authorized() -> Result<(), SpeechError> {
    Err(SpeechError::Audio(
        "microphone not supported on this platform".to_string(),
    ))
}

#[cfg(test)]
#[path = "../tests/unit/permissions_tests.rs"]
mod tests;
