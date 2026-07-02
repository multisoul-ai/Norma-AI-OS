use crate::SpeechError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioInputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub fn resolve_audio_input_device_selection(
    selected_id: Option<&str>,
    devices: &[AudioInputDevice],
) -> Result<Option<AudioInputDevice>, SpeechError> {
    let selected_id = selected_id.map(str::trim).filter(|id| !id.is_empty());
    match selected_id {
        Some(id) => devices
            .iter()
            .find(|device| device.id == id)
            .cloned()
            .map(Some)
            .ok_or_else(|| SpeechError::Audio(format!("selected microphone is unavailable: {id}"))),
        None => Ok(devices
            .iter()
            .find(|device| device.is_default)
            .cloned()
            .or_else(|| devices.first().cloned())),
    }
}

pub fn list_audio_input_devices() -> Result<Vec<AudioInputDevice>, SpeechError> {
    platform::list_audio_input_devices()
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2_av_foundation::{
        AVCaptureDevice, AVCaptureDeviceDiscoverySession, AVCaptureDevicePosition,
        AVCaptureDeviceTypeMicrophone, AVMediaTypeAudio,
    };
    use objc2_foundation::{NSArray, NSString};
    use std::ffi::CStr;

    pub(super) fn list_audio_input_devices() -> Result<Vec<AudioInputDevice>, SpeechError> {
        let media_type = unsafe { AVMediaTypeAudio }
            .ok_or_else(|| SpeechError::Audio("audio media type unavailable".to_string()))?;
        let default_id = unsafe {
            AVCaptureDevice::defaultDeviceWithMediaType(media_type)
                .map(|device| ns_string_to_string(&device.uniqueID()))
        };
        let device_types = NSArray::from_slice(&[unsafe { AVCaptureDeviceTypeMicrophone }]);
        let discovery = unsafe {
            AVCaptureDeviceDiscoverySession::discoverySessionWithDeviceTypes_mediaType_position(
                &device_types,
                Some(media_type),
                AVCaptureDevicePosition::Unspecified,
            )
        };
        let devices = unsafe { discovery.devices() };

        Ok(devices
            .iter()
            .map(|device| {
                let id = unsafe { ns_string_to_string(&device.uniqueID()) };
                let name = unsafe { ns_string_to_string(&device.localizedName()) };
                AudioInputDevice {
                    is_default: default_id.as_deref() == Some(id.as_str()),
                    id,
                    name,
                }
            })
            .collect())
    }

    fn ns_string_to_string(value: &NSString) -> String {
        let ptr = value.UTF8String();
        if ptr.is_null() {
            return String::new();
        }
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;

    pub(super) fn list_audio_input_devices() -> Result<Vec<AudioInputDevice>, SpeechError> {
        Err(SpeechError::Audio(
            "audio input devices not supported on this platform".to_string(),
        ))
    }
}
