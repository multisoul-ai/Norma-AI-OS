//! Native Windows microphone discovery and desktop privacy status.

use crate::{AudioInputDevice, MicrophoneAuthorization, VoiceInputError};
use ::windows::{
    core::{HRESULT, PWSTR},
    Devices::Enumeration::{DeviceAccessInformation, DeviceAccessStatus, DeviceClass},
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::{ERROR_NOT_FOUND, RPC_E_CHANGED_MODE},
        Media::Audio::{
            eCapture, eCommunications, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
        },
        System::{
            Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc},
            Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ},
            WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
        },
    },
};
use std::{marker::PhantomData, rc::Rc};

// Must be dropped on the thread that initialized COM, after all its interfaces.
pub(super) struct Apartment {
    initialized: bool,
    _thread: PhantomData<Rc<()>>,
}

impl Apartment {
    pub(super) fn new() -> Result<Self, VoiceInputError> {
        let initialized = match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
            Ok(()) => true,
            Err(error) if error.code() == RPC_E_CHANGED_MODE => false,
            Err(error) => return Err(audio_error(error)),
        };
        Ok(Self {
            initialized,
            _thread: PhantomData,
        })
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { RoUninitialize() };
        }
    }
}

pub(super) fn audio_error(error: ::windows::core::Error) -> VoiceInputError {
    VoiceInputError::Audio(format!("Windows audio: {error}"))
}

fn take_string(value: PWSTR) -> Result<String, VoiceInputError> {
    let result =
        unsafe { value.to_string() }.map_err(|error| VoiceInputError::Audio(error.to_string()));
    unsafe { CoTaskMemFree(Some(value.0.cast())) };
    result
}

pub(crate) fn list_audio_input_devices() -> Result<Vec<AudioInputDevice>, VoiceInputError> {
    let _apartment = Apartment::new()?;
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(audio_error)?;
        let default_id = match enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications) {
            Ok(device) => Some(take_string(device.GetId().map_err(audio_error)?)?),
            Err(error) if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) => None,
            Err(error) => return Err(audio_error(error)),
        };
        let devices = enumerator
            .EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
            .map_err(audio_error)?;
        let mut result = Vec::new();
        for index in 0..devices.GetCount().map_err(audio_error)? {
            let device = devices.Item(index).map_err(audio_error)?;
            let id = take_string(device.GetId().map_err(audio_error)?)?;
            let store = device.OpenPropertyStore(STGM_READ).map_err(audio_error)?;
            let mut value = store
                .GetValue(&PKEY_Device_FriendlyName)
                .map_err(audio_error)?;
            let name = PropVariantToStringAlloc(&value)
                .map_err(audio_error)
                .and_then(take_string);
            let cleared = PropVariantClear(&mut value).map_err(audio_error);
            let name = name?;
            cleared?;
            result.push(AudioInputDevice {
                is_default: default_id.as_ref() == Some(&id),
                id,
                name,
            });
        }
        Ok(result)
    }
}

pub(super) fn microphone_authorization_status() -> Result<MicrophoneAuthorization, VoiceInputError>
{
    let _apartment = Apartment::new()?;
    let access = DeviceAccessInformation::CreateFromDeviceClass(DeviceClass::AudioCapture)
        .map_err(audio_error)?;
    match access.CurrentStatus().map_err(audio_error)? {
        DeviceAccessStatus::Allowed => Ok(MicrophoneAuthorization::Authorized),
        DeviceAccessStatus::DeniedByUser => Ok(MicrophoneAuthorization::Denied),
        DeviceAccessStatus::DeniedBySystem => Ok(MicrophoneAuthorization::Restricted),
        DeviceAccessStatus::Unspecified => Ok(MicrophoneAuthorization::NotDetermined),
        status => Err(VoiceInputError::Audio(format!(
            "unknown Windows microphone access status: {}",
            status.0
        ))),
    }
}
