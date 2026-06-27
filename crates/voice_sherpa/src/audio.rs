use crate::SpeechError;
use crossbeam_channel::Sender;

const INPUT_BUS: usize = 0;
const TAP_BUFFER_SIZE: u32 = 1_600; // 100ms at 16kHz
const SAMPLE_RATE: f64 = 16_000.0;

pub(crate) struct AudioCapture {
    #[cfg(target_os = "macos")]
    inner: macos::MacosAudioCapture,
}

impl AudioCapture {
    pub(crate) fn start(sender: Sender<Vec<f32>>) -> Result<Self, SpeechError> {
        #[cfg(target_os = "macos")]
        {
            Ok(Self {
                inner: macos::MacosAudioCapture::start(sender)?,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = sender;
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
mod macos {
    use super::*;
    use block2::RcBlock;
    use objc2::{rc::Retained, AnyThread};
    use objc2_avf_audio::{
        AVAudioEngine, AVAudioFormat, AVAudioInputNode, AVAudioPCMBuffer, AVAudioTime,
    };
    use std::ptr::NonNull;

    type TapBlock = RcBlock<dyn Fn(NonNull<AVAudioPCMBuffer>, NonNull<AVAudioTime>)>;

    pub(super) struct MacosAudioCapture {
        engine: Retained<AVAudioEngine>,
        input_node: Retained<AVAudioInputNode>,
        _tap_block: TapBlock,
        tap_installed: bool,
    }

    impl MacosAudioCapture {
        pub(super) fn start(sender: Sender<Vec<f32>>) -> Result<Self, SpeechError> {
            let engine = unsafe { AVAudioEngine::new() };
            let input_node = unsafe { engine.inputNode() };

            // Request 16kHz mono f32 so AVAudioEngine resamples from hardware rate.
            let format = unsafe {
                AVAudioFormat::initStandardFormatWithSampleRate_channels(
                    AVAudioFormat::alloc(),
                    SAMPLE_RATE,
                    1,
                )
            }
            .ok_or_else(|| SpeechError::Audio("could not create 16kHz audio format".to_string()))?;

            let tap_block = RcBlock::new(
                move |buffer: NonNull<AVAudioPCMBuffer>, _: NonNull<AVAudioTime>| {
                    let buf = unsafe { buffer.as_ref() };
                    let frame_count = unsafe { buf.frameLength() } as usize;
                    if frame_count == 0 {
                        return;
                    }
                    // Extract f32 samples from float channel data pointer.
                    let samples: Vec<f32> = unsafe {
                        let channels = buf.floatChannelData();
                        if channels.is_null() {
                            return;
                        }
                        // *channels is NonNull<c_float>; call .as_ptr() to get *const f32.
                        let channel_ptr = (*channels).as_ptr();
                        std::slice::from_raw_parts(channel_ptr, frame_count).to_vec()
                    };
                    // Use try_send to avoid blocking the real-time audio thread.
                    let _ = sender.try_send(samples);
                },
            );

            unsafe {
                input_node.installTapOnBus_bufferSize_format_block(
                    INPUT_BUS,
                    TAP_BUFFER_SIZE,
                    Some(&format),
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
pub fn audio_capture_for_test(
    sender: crossbeam_channel::Sender<Vec<f32>>,
) -> Result<AudioCapture, SpeechError> {
    AudioCapture::start(sender)
}

#[cfg(test)]
#[path = "../tests/unit/audio_tests.rs"]
mod tests;
