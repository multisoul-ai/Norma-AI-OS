use crate::session::{AudioQueueStats, PcmEvent};
use crate::SpeechError;
use crossbeam_channel::Sender;
use std::sync::Arc;

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, Eq, PartialEq)]
struct CoreAudioInputDevice {
    uid: String,
    device_id: objc2_core_audio::AudioObjectID,
}

const INPUT_BUS: usize = 0;
const TAP_BUFFER_SIZE: u32 = 1_600; // requested tap size in frames (hint; HW may differ)
const SAMPLE_RATE: f64 = 16_000.0; // sherpa Paraformer target rate

pub(crate) struct AudioCapture {
    #[cfg(target_os = "macos")]
    inner: macos::MacosAudioCapture,
}

impl AudioCapture {
    pub(crate) fn start(
        sender: Sender<PcmEvent>,
        stats: Arc<AudioQueueStats>,
        input_device_id: Option<&str>,
    ) -> Result<Self, SpeechError> {
        #[cfg(target_os = "macos")]
        {
            Ok(Self {
                inner: macos::MacosAudioCapture::start(sender, stats, input_device_id)?,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = sender;
            let _ = stats;
            let _ = input_device_id;
            Err(SpeechError::Audio(
                "audio not supported on this platform".to_string(),
            ))
        }
    }

    pub(crate) fn stop(self) {
        #[cfg(target_os = "macos")]
        self.inner.stop();
    }
}

#[cfg(target_os = "macos")]
fn resolve_core_audio_input_device_id(
    selected_uid: &str,
    devices: &[CoreAudioInputDevice],
) -> Result<objc2_core_audio::AudioObjectID, SpeechError> {
    let selected_uid = selected_uid.trim();
    devices
        .iter()
        .find(|device| device.uid == selected_uid)
        .map(|device| device.device_id)
        .ok_or_else(|| {
            SpeechError::Audio(format!(
                "selected microphone is unavailable: {selected_uid}"
            ))
        })
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::resample::LinearResampler;
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2_audio_toolbox::{
        kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, AudioUnitSetProperty,
    };
    use objc2_avf_audio::{AVAudioEngine, AVAudioInputNode, AVAudioPCMBuffer, AVAudioTime};
    use objc2_core_audio::{
        kAudioDevicePropertyDeviceUID, kAudioHardwarePropertyDevices,
        kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
        AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
        AudioObjectPropertyAddress,
    };
    use objc2_core_foundation::{CFRetained, CFString};
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr;
    use std::ptr::NonNull;

    type TapBlock = RcBlock<dyn Fn(NonNull<AVAudioPCMBuffer>, NonNull<AVAudioTime>)>;

    pub(super) struct MacosAudioCapture {
        engine: Retained<AVAudioEngine>,
        input_node: Retained<AVAudioInputNode>,
        _tap_block: TapBlock,
        tap_installed: bool,
    }

    impl MacosAudioCapture {
        pub(super) fn start(
            sender: Sender<PcmEvent>,
            stats: Arc<AudioQueueStats>,
            input_device_id: Option<&str>,
        ) -> Result<Self, SpeechError> {
            let engine = unsafe { AVAudioEngine::new() };
            let input_node = unsafe { engine.inputNode() };
            let input_device_id = input_device_id.map(str::trim).filter(|id| !id.is_empty());
            if let Some(input_device_id) = input_device_id {
                let devices = list_core_audio_devices()?;
                let device_id = resolve_core_audio_input_device_id(input_device_id, &devices)?;
                set_audio_unit_input_device(&input_node, device_id)?;
            }

            // AVAudioEngine will NOT resample on an inputNode tap: the tap format
            // must equal the hardware output format, otherwise it throws
            // "Input HW format and tap format not matching". Tap at the native
            // rate and downsample to 16kHz mono in the callback instead.
            let hw_format = unsafe { input_node.outputFormatForBus(INPUT_BUS) };
            let hw_sample_rate = unsafe { hw_format.sampleRate() };
            if hw_sample_rate <= 0.0 {
                return Err(SpeechError::Audio(
                    "microphone reported an invalid hardware sample rate".to_string(),
                ));
            }

            let resampler = RefCell::new(LinearResampler::new(hw_sample_rate, SAMPLE_RATE));
            let tap_block = RcBlock::new(
                move |buffer: NonNull<AVAudioPCMBuffer>, _: NonNull<AVAudioTime>| {
                    let buf = unsafe { buffer.as_ref() };
                    let frame_count = unsafe { buf.frameLength() } as usize;
                    if frame_count == 0 {
                        return;
                    }
                    // Extract channel 0 f32 samples at the hardware rate.
                    let samples: Vec<f32> = unsafe {
                        let channels = buf.floatChannelData();
                        if channels.is_null() {
                            return;
                        }
                        // *channels is NonNull<c_float>; call .as_ptr() to get *const f32.
                        let channel_ptr = (*channels).as_ptr();
                        std::slice::from_raw_parts(channel_ptr, frame_count).to_vec()
                    };
                    let resampled = resampler.borrow_mut().process(&samples);
                    if resampled.is_empty() {
                        return;
                    }
                    // Use try_send to avoid blocking the real-time audio thread.
                    match sender.try_send(PcmEvent::Samples(resampled)) {
                        Ok(()) => stats.record_sent_chunk(),
                        Err(crossbeam_channel::TrySendError::Full(_)) => {
                            stats.record_dropped_chunk();
                        }
                        Err(crossbeam_channel::TrySendError::Disconnected(_)) => {}
                    }
                },
            );

            unsafe {
                input_node.installTapOnBus_bufferSize_format_block(
                    INPUT_BUS,
                    TAP_BUFFER_SIZE,
                    Some(&hw_format),
                    RcBlock::as_ptr(&tap_block),
                );
                engine.prepare();
                engine
                    .startAndReturnError()
                    .map_err(|e| SpeechError::Audio(e.localizedDescription().to_string()))?;
            }

            Ok(Self {
                engine,
                input_node,
                _tap_block: tap_block,
                tap_installed: true,
            })
        }

        pub(super) fn stop(mut self) {
            self.stop_audio();
        }

        fn stop_audio(&mut self) {
            if self.tap_installed {
                unsafe { self.input_node.removeTapOnBus(INPUT_BUS) };
                self.tap_installed = false;
            }
            unsafe { self.engine.stop() };
        }
    }

    fn list_core_audio_devices() -> Result<Vec<CoreAudioInputDevice>, SpeechError> {
        let mut address = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyDevices,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut data_size = 0u32;
        let status = unsafe {
            AudioObjectGetPropertyDataSize(
                kAudioObjectSystemObject as AudioObjectID,
                NonNull::from(&mut address),
                0,
                ptr::null(),
                NonNull::from(&mut data_size),
            )
        };
        if status != 0 {
            return Err(core_audio_error(
                "could not query audio device list size",
                status,
            ));
        }

        let count = data_size as usize / size_of::<AudioObjectID>();
        let mut device_ids = vec![0 as AudioObjectID; count];
        let status = unsafe {
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject as AudioObjectID,
                NonNull::from(&mut address),
                0,
                ptr::null(),
                NonNull::from(&mut data_size),
                NonNull::new(device_ids.as_mut_ptr().cast::<c_void>())
                    .expect("device ids buffer should not be null"),
            )
        };
        if status != 0 {
            return Err(core_audio_error(
                "could not query audio device list",
                status,
            ));
        }

        device_ids
            .into_iter()
            .map(|device_id| {
                Ok(CoreAudioInputDevice {
                    uid: core_audio_device_uid(device_id)?,
                    device_id,
                })
            })
            .collect()
    }

    fn core_audio_device_uid(device_id: AudioObjectID) -> Result<String, SpeechError> {
        let mut address = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyDeviceUID,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut uid_ref: *const CFString = ptr::null();
        let mut data_size = size_of::<*const CFString>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                device_id,
                NonNull::from(&mut address),
                0,
                ptr::null(),
                NonNull::from(&mut data_size),
                NonNull::new((&mut uid_ref as *mut *const CFString).cast::<c_void>())
                    .expect("uid output pointer should not be null"),
            )
        };
        if status != 0 {
            return Err(core_audio_error(
                "could not query audio device unique id",
                status,
            ));
        }
        let uid_ref = NonNull::new(uid_ref.cast_mut())
            .ok_or_else(|| SpeechError::Audio("audio device unique id is null".to_string()))?;
        let uid = unsafe { CFRetained::from_raw(uid_ref) };
        Ok(uid.to_string())
    }

    fn set_audio_unit_input_device(
        input_node: &AVAudioInputNode,
        device_id: AudioObjectID,
    ) -> Result<(), SpeechError> {
        let audio_unit = unsafe { input_node.audioUnit() };
        let status = unsafe {
            AudioUnitSetProperty(
                audio_unit,
                kAudioOutputUnitProperty_CurrentDevice,
                kAudioUnitScope_Global,
                0,
                (&device_id as *const AudioObjectID).cast::<c_void>(),
                size_of::<AudioObjectID>() as u32,
            )
        };
        if status != 0 {
            return Err(core_audio_error(
                "could not select microphone for audio unit",
                status,
            ));
        }
        Ok(())
    }

    fn core_audio_error(action: &str, status: i32) -> SpeechError {
        SpeechError::Audio(format!("{action}: OSStatus {status}"))
    }

    impl Drop for MacosAudioCapture {
        fn drop(&mut self) {
            // Ensures the tap is removed even if stop() was never called explicitly,
            // preventing a crash when the engine is deallocated with an active tap.
            self.stop_audio();
        }
    }
}

/// Exposed for integration testing only.
#[cfg(test)]
fn audio_capture_for_test(
    sender: crossbeam_channel::Sender<PcmEvent>,
) -> Result<AudioCapture, SpeechError> {
    AudioCapture::start(sender, Arc::new(AudioQueueStats::default()), None)
}

#[cfg(test)]
#[path = "../tests/unit/audio_tests.rs"]
mod tests;
