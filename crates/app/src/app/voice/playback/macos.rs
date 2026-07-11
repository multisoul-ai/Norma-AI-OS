use super::{pcm_chunk_duration, AssistantAudioSink};
use audio::PcmAudioChunk;
use block2::RcBlock;
use objc2::rc::autoreleasepool;
use objc2::AnyThread;
use objc2_avf_audio::{
    AVAudioCommonFormat, AVAudioEngine, AVAudioFormat, AVAudioPCMBuffer, AVAudioPlayerNode,
    AVAudioPlayerNodeCompletionCallbackType,
};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

enum PlaybackCommand {
    Play {
        chunk: PcmAudioChunk,
        result_tx: mpsc::Sender<Result<(), String>>,
        completion_tx: async_channel::Sender<()>,
    },
    Stop,
    Shutdown,
}

pub(super) struct MacosAssistantAudioSink {
    command_tx: mpsc::Sender<PlaybackCommand>,
    thread: Option<JoinHandle<()>>,
}

impl MacosAssistantAudioSink {
    pub(super) fn new() -> Result<Self, String> {
        let (command_tx, command_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("assistant_audio_playback".to_string())
            .spawn(move || autoreleasepool(|_| playback_loop(command_rx)))
            .map_err(|error| format!("could not spawn assistant playback thread: {error}"))?;
        Ok(Self {
            command_tx,
            thread: Some(thread),
        })
    }
}

impl AssistantAudioSink for MacosAssistantAudioSink {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk) -> Result<Duration, String> {
        let (completion_tx, _completion_rx) = async_channel::bounded(1);
        self.write_chunk_with_completion(chunk, completion_tx)
    }

    fn write_chunk_with_completion(
        &mut self,
        chunk: &PcmAudioChunk,
        completion_tx: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        let duration = pcm_chunk_duration(chunk);
        let (result_tx, result_rx) = mpsc::channel();
        self.command_tx
            .send(PlaybackCommand::Play {
                chunk: chunk.clone(),
                result_tx,
                completion_tx,
            })
            .map_err(|_| "assistant playback thread is unavailable".to_string())?;
        result_rx
            .recv_timeout(COMMAND_TIMEOUT)
            .map_err(|_| "assistant playback scheduling timed out".to_string())??;
        Ok(duration)
    }

    fn stop(&mut self) {
        let _ = self.command_tx.send(PlaybackCommand::Stop);
    }
}

impl Drop for MacosAssistantAudioSink {
    fn drop(&mut self) {
        let _ = self.command_tx.send(PlaybackCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn playback_loop(command_rx: mpsc::Receiver<PlaybackCommand>) {
    let engine = unsafe { AVAudioEngine::new() };
    let player = unsafe { AVAudioPlayerNode::new() };
    unsafe { engine.attachNode(&player) };
    let mut connected_sample_rate = None;

    while let Ok(command) = command_rx.recv() {
        match command {
            PlaybackCommand::Play {
                chunk,
                result_tx,
                completion_tx,
            } => {
                let result = schedule_chunk(
                    &engine,
                    &player,
                    &chunk,
                    &mut connected_sample_rate,
                    completion_tx,
                );
                let _ = result_tx.send(result);
            }
            PlaybackCommand::Stop => unsafe { player.stop() },
            PlaybackCommand::Shutdown => {
                unsafe {
                    player.stop();
                    engine.stop();
                }
                break;
            }
        }
    }
}

fn schedule_chunk(
    engine: &AVAudioEngine,
    player: &AVAudioPlayerNode,
    chunk: &PcmAudioChunk,
    connected_sample_rate: &mut Option<u32>,
    completion_tx: async_channel::Sender<()>,
) -> Result<(), String> {
    let frame_count = u32::try_from(chunk.bytes.len() / 2)
        .map_err(|_| "assistant PCM chunk is too large".to_string())?;
    let format = unsafe {
        AVAudioFormat::initWithCommonFormat_sampleRate_channels_interleaved(
            AVAudioFormat::alloc(),
            AVAudioCommonFormat::PCMFormatInt16,
            f64::from(chunk.sample_rate_hz),
            u32::from(chunk.channels),
            false,
        )
    }
    .ok_or_else(|| "could not create assistant PCM format".to_string())?;
    let buffer = unsafe {
        AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(
            AVAudioPCMBuffer::alloc(),
            &format,
            frame_count,
        )
    }
    .ok_or_else(|| "could not allocate assistant PCM buffer".to_string())?;

    let channel_data = unsafe { buffer.int16ChannelData() };
    if channel_data.is_null() {
        return Err("assistant PCM buffer has no int16 channel data".to_string());
    }
    let destination =
        unsafe { std::slice::from_raw_parts_mut((*channel_data).as_ptr(), frame_count as usize) };
    for (destination_sample, source_bytes) in
        destination.iter_mut().zip(chunk.bytes.chunks_exact(2))
    {
        *destination_sample = i16::from_le_bytes([source_bytes[0], source_bytes[1]]);
    }
    unsafe { buffer.setFrameLength(frame_count) };

    if *connected_sample_rate != Some(chunk.sample_rate_hz) {
        unsafe {
            player.stop();
            engine.disconnectNodeOutput(player);
            let mixer = engine.mainMixerNode();
            engine.connect_to_format(player, &mixer, Some(&format));
        }
        *connected_sample_rate = Some(chunk.sample_rate_hz);
    }

    unsafe {
        if !engine.isRunning() {
            engine.prepare();
            engine
                .startAndReturnError()
                .map_err(|error| error.localizedDescription().to_string())?;
        }
        let completion_handler = RcBlock::new(
            move |callback_type: AVAudioPlayerNodeCompletionCallbackType| {
                if callback_type == AVAudioPlayerNodeCompletionCallbackType::DataPlayedBack {
                    let _ = completion_tx.try_send(());
                }
            },
        );
        player.scheduleBuffer_completionCallbackType_completionHandler(
            &buffer,
            AVAudioPlayerNodeCompletionCallbackType::DataPlayedBack,
            RcBlock::as_ptr(&completion_handler).cast(),
        );
        if !player.isPlaying() {
            player.play();
        }
    }
    Ok(())
}
