use crate::session::{AudioQueueStats, PcmEvent};
use crate::SpeechError;
use crossbeam_channel::Sender;
use realtime::{PcmAudioChunk, PcmFormat};
use std::sync::Arc;

const INPUT_BUS: usize = 0;
const TAP_BUFFER_SIZE: u32 = 1_600; // requested tap size in frames (hint; HW may differ)
pub(crate) const RECOGNIZER_SAMPLE_RATE_HZ: u32 = 16_000;
pub(crate) const PCM_STREAM_SAMPLE_RATE_HZ: u32 = 24_000;

pub(crate) struct AudioCapture {
    #[cfg(target_os = "macos")]
    inner: macos::MacosAudioCapture,
}

impl AudioCapture {
    pub(crate) fn start(
        sender: Sender<PcmEvent>,
        stats: Arc<AudioQueueStats>,
        input_device_id: Option<&str>,
        target_sample_rate_hz: u32,
    ) -> Result<Self, SpeechError> {
        #[cfg(target_os = "macos")]
        {
            Ok(Self {
                inner: macos::MacosAudioCapture::start(
                    sender,
                    stats,
                    input_device_id,
                    target_sample_rate_hz,
                )?,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = sender;
            let _ = stats;
            let _ = input_device_id;
            let _ = target_sample_rate_hz;
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

pub(crate) fn pcm16_bytes_from_f32_samples(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&pcm16_sample_from_f32(*sample).to_le_bytes());
    }
    bytes
}

pub(crate) fn pcm_stream_chunk_from_samples(samples: &[f32]) -> Option<PcmAudioChunk> {
    if samples.is_empty() {
        return None;
    }
    PcmAudioChunk::new(
        PcmFormat::Pcm16,
        PCM_STREAM_SAMPLE_RATE_HZ,
        1,
        pcm16_bytes_from_f32_samples(samples),
    )
    .ok()
}

fn pcm16_sample_from_f32(sample: f32) -> i16 {
    if sample <= -1.0 {
        return i16::MIN;
    }
    if sample >= 1.0 {
        return i16::MAX;
    }
    if !sample.is_finite() {
        return 0;
    }
    let scale = if sample < 0.0 { 32_768.0 } else { 32_767.0 };
    (sample * scale).round() as i16
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::resample::LinearResampler;
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2_avf_audio::{AVAudioEngine, AVAudioInputNode, AVAudioPCMBuffer, AVAudioTime};
    use std::cell::RefCell;
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
            _input_device_id: Option<&str>,
            target_sample_rate_hz: u32,
        ) -> Result<Self, SpeechError> {
            let engine = unsafe { AVAudioEngine::new() };
            let input_node = unsafe { engine.inputNode() };

            // AVAudioEngine will NOT resample on an inputNode tap: the tap format
            // must equal the hardware output format, otherwise it throws
            // "Input HW format and tap format not matching". Tap at the native
            // rate and resample to the target mono rate in the callback instead.
            let hw_format = unsafe { input_node.outputFormatForBus(INPUT_BUS) };
            let hw_sample_rate = unsafe { hw_format.sampleRate() };
            if hw_sample_rate <= 0.0 {
                return Err(SpeechError::Audio(
                    "microphone reported an invalid hardware sample rate".to_string(),
                ));
            }

            let resampler = RefCell::new(LinearResampler::new(
                hw_sample_rate,
                f64::from(target_sample_rate_hz),
            ));
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
    AudioCapture::start(
        sender,
        Arc::new(AudioQueueStats::default()),
        None,
        RECOGNIZER_SAMPLE_RATE_HZ,
    )
}

#[cfg(test)]
#[path = "../tests/unit/audio_tests.rs"]
mod audio_tests;
