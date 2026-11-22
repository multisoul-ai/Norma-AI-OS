use crate::VoiceInputError;
use crate::{AudioQueueStats, PcmEvent};
use audio::{PcmAudioChunk, PcmFormat};
use crossbeam_channel::Sender;
use std::sync::Arc;

const INPUT_BUS: usize = 0;
const TAP_BUFFER_SIZE: u32 = 1_600; // requested tap size in frames (hint; HW may differ)
pub const RECOGNIZER_SAMPLE_RATE_HZ: u32 = 16_000;
pub const PCM_STREAM_SAMPLE_RATE_HZ: u32 = 16_000;

pub struct AudioCapture {
    #[cfg(target_os = "macos")]
    inner: macos::MacosAudioCapture,
}

impl AudioCapture {
    pub fn start(
        sender: Sender<PcmEvent>,
        stats: Arc<AudioQueueStats>,
        input_device_id: Option<&str>,
        target_sample_rate_hz: u32,
    ) -> Result<Self, VoiceInputError> {
        Self::start_with_voice_processing(
            sender,
            stats,
            input_device_id,
            target_sample_rate_hz,
            false,
        )
    }

    pub(crate) fn start_with_voice_processing(
        sender: Sender<PcmEvent>,
        stats: Arc<AudioQueueStats>,
        input_device_id: Option<&str>,
        target_sample_rate_hz: u32,
        voice_processing: bool,
    ) -> Result<Self, VoiceInputError> {
        if crate::permissions::fixture_replaces_microphone() {
            return Err(VoiceInputError::Audio(
                "E2E audio fixture is active; this capture path does not support it".to_string(),
            ));
        }
        #[cfg(target_os = "macos")]
        {
            Ok(Self {
                inner: macos::MacosAudioCapture::start(
                    sender,
                    stats,
                    input_device_id,
                    target_sample_rate_hz,
                    voice_processing,
                )?,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = sender;
            let _ = stats;
            let _ = input_device_id;
            let _ = target_sample_rate_hz;
            let _ = voice_processing;
            Err(VoiceInputError::Audio(
                "audio not supported on this platform".to_string(),
            ))
        }
    }

    pub fn stop(self) {
        #[cfg(target_os = "macos")]
        self.inner.stop();
    }

    pub(crate) fn pump_playback(
        &mut self,
        stop_rx: std::sync::mpsc::Receiver<()>,
        play_rx: std::sync::mpsc::Receiver<crate::duplex_playback::DuplexCommand>,
    ) {
        #[cfg(target_os = "macos")]
        self.inner.pump_playback(stop_rx, play_rx);
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (stop_rx, play_rx);
        }
    }
}

pub fn pcm16_bytes_from_f32_samples(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&pcm16_sample_from_f32(*sample).to_le_bytes());
    }
    bytes
}

pub fn pcm_stream_chunk_from_samples(samples: &[f32]) -> Option<PcmAudioChunk> {
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
    use objc2::{rc::Retained, AllocAnyThread};
    use objc2_avf_audio::{
        AVAudioCommonFormat, AVAudioEngine, AVAudioFormat, AVAudioInputNode, AVAudioMixerNode,
        AVAudioPCMBuffer, AVAudioPlayerNode, AVAudioPlayerNodeCompletionCallbackType, AVAudioTime,
    };
    use std::cell::RefCell;
    use std::ptr::NonNull;

    type TapBlock = RcBlock<dyn Fn(NonNull<AVAudioPCMBuffer>, NonNull<AVAudioTime>)>;

    pub(super) struct MacosAudioCapture {
        engine: Retained<AVAudioEngine>,
        input_node: Retained<AVAudioInputNode>,
        player: Option<Retained<AVAudioPlayerNode>>,
        _tap_block: TapBlock,
        tap_installed: bool,
    }

    impl MacosAudioCapture {
        pub(super) fn start(
            sender: Sender<PcmEvent>,
            stats: Arc<AudioQueueStats>,
            _input_device_id: Option<&str>,
            target_sample_rate_hz: u32,
            voice_processing: bool,
        ) -> Result<Self, VoiceInputError> {
            let engine = unsafe { AVAudioEngine::new() };
            let input_node = unsafe { engine.inputNode() };
            if voice_processing {
                unsafe { input_node.setVoiceProcessingEnabled_error(true) }.map_err(|error| {
                    VoiceInputError::Audio(format!(
                        "cannot enable acoustic echo cancellation: {}",
                        error.localizedDescription()
                    ))
                })?;
            }

            // AVAudioEngine will NOT resample on a raw inputNode tap: the tap format
            // must equal the hardware output format, otherwise it throws
            // "Input HW format and tap format not matching". Tap at the native
            // rate and resample to the target mono rate in the callback instead.
            let mut player = None;
            let hw_format = if voice_processing {
                // 统一 VPIO 两侧为 48 kHz 单声道。麦克风经静音 mixer 进入同一条输出，
                // 播放节点走可听的主 mixer。回声消除因此听到正在播放的声音。
                let format = unsafe {
                    AVAudioFormat::initStandardFormatWithSampleRate_channels(
                        AVAudioFormat::alloc(),
                        48_000.0,
                        1,
                    )
                }
                .ok_or_else(|| VoiceInputError::Audio("invalid voice processing format".into()))?;
                let playback = unsafe { AVAudioPlayerNode::new() };
                unsafe {
                    // 先实例化输出节点，再连接 mixer；反序会导致重开后没有 PCM。
                    let output = engine.outputNode();
                    let main = engine.mainMixerNode();
                    main.setOutputVolume(1.0);
                    let mic_mixer = AVAudioMixerNode::new();
                    engine.attachNode(&mic_mixer);
                    engine.attachNode(&playback);
                    mic_mixer.setOutputVolume(0.0);
                    engine.connect_to_format(&input_node, &mic_mixer, Some(&format));
                    engine.connect_to_format(&mic_mixer, &main, Some(&format));
                    engine.connect_to_format(&playback, &main, Some(&format));
                    engine.connect_to_format(&main, &output, Some(&format));
                }
                player = Some(playback);
                format
            } else {
                unsafe { input_node.outputFormatForBus(INPUT_BUS) }
            };
            let hw_sample_rate = unsafe { hw_format.sampleRate() };
            if hw_sample_rate <= 0.0 {
                return Err(VoiceInputError::Audio(
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
                    .map_err(|e| VoiceInputError::Audio(e.localizedDescription().to_string()))?;
            }

            Ok(Self {
                engine,
                input_node,
                player,
                _tap_block: tap_block,
                tap_installed: true,
            })
        }

        pub(super) fn pump_playback(
            &mut self,
            stop_rx: std::sync::mpsc::Receiver<()>,
            play_rx: std::sync::mpsc::Receiver<crate::duplex_playback::DuplexCommand>,
        ) {
            let Some(player) = self.player.clone() else {
                let _ = stop_rx.recv();
                return;
            };
            loop {
                match stop_rx.try_recv() {
                    Ok(()) | Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
                match play_rx.recv_timeout(std::time::Duration::from_millis(20)) {
                    Ok(crate::duplex_playback::DuplexCommand::Play {
                        pcm,
                        started,
                        done,
                        result,
                    }) => {
                        let scheduled = schedule_player(&self.engine, &player, &pcm, started, done);
                        let _ = result.send(scheduled);
                    }
                    Ok(crate::duplex_playback::DuplexCommand::StopPlayer(done)) => {
                        unsafe { player.stop() };
                        let _ = done.send(Ok(()));
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = stop_rx.recv();
                        break;
                    }
                }
            }
        }

        pub(super) fn stop(mut self) {
            self.stop_audio();
        }

        fn stop_audio(&mut self) {
            if !self.tap_installed {
                return;
            }
            unsafe { self.input_node.removeTapOnBus(INPUT_BUS) };
            self.tap_installed = false;
            unsafe { self.engine.stop() };
            if unsafe { self.input_node.isVoiceProcessingEnabled() } {
                if let Err(error) =
                    unsafe { self.input_node.setVoiceProcessingEnabled_error(false) }
                {
                    tracing::warn!(target: "norma_voice", reason = %error.localizedDescription(),
                        "could not release voice processing IO after capture stopped");
                }
            }
        }
    }

    fn schedule_player(
        engine: &AVAudioEngine,
        player: &AVAudioPlayerNode,
        pcm: &audio::PcmAudioChunk,
        started: async_channel::Sender<()>,
        done: async_channel::Sender<()>,
    ) -> Result<(), String> {
        let samples = mono_f32_at_output_rate(pcm)?;
        let frame_count = u32::try_from(samples.len())
            .map_err(|_| "assistant PCM chunk is too large".to_string())?;
        let format = unsafe {
            AVAudioFormat::initWithCommonFormat_sampleRate_channels_interleaved(
                AVAudioFormat::alloc(),
                AVAudioCommonFormat::PCMFormatFloat32,
                f64::from(crate::duplex_playback::DUPLEX_OUTPUT_RATE_HZ),
                1,
                false,
            )
        }
        .ok_or_else(|| "could not create duplex playback format".to_string())?;
        let buffer = unsafe {
            AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(
                AVAudioPCMBuffer::alloc(),
                &format,
                frame_count,
            )
        }
        .ok_or_else(|| "could not allocate duplex playback buffer".to_string())?;
        let channel_data = unsafe { buffer.floatChannelData() };
        if channel_data.is_null() {
            return Err("duplex playback buffer has no float channel data".to_string());
        }
        let destination =
            unsafe { std::slice::from_raw_parts_mut((*channel_data).as_ptr(), samples.len()) };
        destination.copy_from_slice(&samples);
        unsafe { buffer.setFrameLength(frame_count) };
        unsafe {
            if !engine.isRunning() {
                engine.prepare();
                engine
                    .startAndReturnError()
                    .map_err(|error| error.localizedDescription().to_string())?;
            }
            let completion = RcBlock::new(
                move |callback_type: AVAudioPlayerNodeCompletionCallbackType| {
                    if callback_type == AVAudioPlayerNodeCompletionCallbackType::DataPlayedBack {
                        let _ = done.try_send(());
                    }
                },
            );
            player.scheduleBuffer_completionCallbackType_completionHandler(
                &buffer,
                AVAudioPlayerNodeCompletionCallbackType::DataPlayedBack,
                RcBlock::as_ptr(&completion).cast(),
            );
            if !player.isPlaying() {
                player.play();
            }
            let _ = started.try_send(());
        }
        Ok(())
    }

    fn mono_f32_at_output_rate(pcm: &audio::PcmAudioChunk) -> Result<Vec<f32>, String> {
        if pcm.channels != 1 || !pcm.bytes.len().is_multiple_of(2) || pcm.sample_rate_hz == 0 {
            return Err("duplex playback expects mono PCM16".to_string());
        }
        let mut input = Vec::with_capacity(pcm.bytes.len() / 2);
        for sample in pcm.bytes.chunks_exact(2) {
            input.push(f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32_768.0);
        }
        if pcm.sample_rate_hz == crate::duplex_playback::DUPLEX_OUTPUT_RATE_HZ {
            return Ok(input);
        }
        Ok(crate::resample::LinearResampler::new(
            f64::from(pcm.sample_rate_hz),
            f64::from(crate::duplex_playback::DUPLEX_OUTPUT_RATE_HZ),
        )
        .process(&input))
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
) -> Result<AudioCapture, VoiceInputError> {
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
