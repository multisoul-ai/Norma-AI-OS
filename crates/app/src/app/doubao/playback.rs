use crate::app::voice_window_inset::{
    PcmEnergyTimeline, TtsPlaybackEvent, VoiceWindowInsetEventSink,
};
use audio::PcmAudioChunk;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

// TTS admits at most 30 s of PCM; 100 ms coalescing produces at most 301 buffers.
const MAX_PENDING_COMPLETIONS: usize = 512;
const COMPLETION_GRACE: Duration = Duration::from_secs(5);

struct PendingCompletion {
    receiver: async_channel::Receiver<()>,
    deadline: Instant,
}

enum PlaybackControl {
    Interrupt,
    End,
    Finish {
        generation: u64,
        done: async_channel::Sender<()>,
    },
}

pub(super) struct PlaybackQueue {
    audio: crossbeam_channel::Sender<(u64, PcmAudioChunk)>,
    control: crossbeam_channel::Sender<PlaybackControl>,
    epoch: Arc<AtomicU64>,
    pub(super) errors: async_channel::Receiver<String>,
}

impl PlaybackQueue {
    pub(super) fn generation(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub(super) fn enqueue_for(&self, generation: u64, pcm: PcmAudioChunk) -> Result<(), String> {
        if generation != self.generation() {
            return Ok(());
        }
        self.audio
            .try_send((generation, pcm))
            .map_err(|_| "豆包音频播放跟不上实时语速，请检查输出设备".to_string())
    }

    pub(super) fn finish_for(
        &self,
        generation: u64,
    ) -> Result<async_channel::Receiver<()>, String> {
        let (done, receiver) = async_channel::bounded(1);
        self.control
            .send(PlaybackControl::Finish { generation, done })
            .map_err(|_| "豆包音频播放器已结束".to_string())?;
        Ok(receiver)
    }

    pub(super) fn interrupt(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let _ = self.control.send(PlaybackControl::Interrupt);
    }

    pub(super) fn end(&self) {
        self.interrupt();
        let _ = self.control.send(PlaybackControl::End);
    }
}

pub(super) fn start_playback(events: VoiceWindowInsetEventSink) -> PlaybackQueue {
    start_playback_with(
        crate::app::voice::playback::default_assistant_audio_sink,
        events,
    )
}

pub(super) fn start_playback_with(
    make_sink: impl FnOnce() -> Box<dyn crate::app::voice::playback::AssistantAudioSink>
        + Send
        + 'static,
    events: VoiceWindowInsetEventSink,
) -> PlaybackQueue {
    let (audio_tx, audio_rx) = crossbeam_channel::bounded(16);
    let (control_tx, control_rx) = crossbeam_channel::unbounded();
    let (error_tx, error_rx) = async_channel::bounded(1);
    let epoch = Arc::new(AtomicU64::new(0));
    let epoch_worker = Arc::clone(&epoch);
    std::thread::spawn(move || {
        let mut sink = make_sink();
        let tick = crossbeam_channel::tick(Duration::from_millis(12));
        let idle = crossbeam_channel::never::<Instant>();
        let paused_audio = crossbeam_channel::never::<(u64, PcmAudioChunk)>();
        let mut pending = Vec::<PendingCompletion>::new();
        let mut finishing: Option<(u64, async_channel::Sender<()>)> = None;
        let mut started = false;
        let mut playback_tail = Instant::now();
        loop {
            let completion_tick = if pending.is_empty() { &idle } else { &tick };
            let next_audio = if pending.len() < MAX_PENDING_COMPLETIONS {
                &audio_rx
            } else {
                &paused_audio
            };
            crossbeam_channel::select_biased! {
                recv(control_rx) -> command => match command {
                    Ok(PlaybackControl::Interrupt) => {
                        sink.stop();
                        pending.clear();
                        finishing = None;
                        playback_tail = Instant::now();
                        if started {
                            events.push(TtsPlaybackEvent::Interrupted);
                            started = false;
                        }
                    }
                    Ok(PlaybackControl::End) | Err(_) => break,
                    Ok(PlaybackControl::Finish { generation, done }) => {
                        if generation == epoch_worker.load(Ordering::Acquire) {
                            finishing = Some((generation, done));
                        }
                    }
                },
                recv(next_audio) -> command => match command {
                    Ok((generation, pcm)) if generation == epoch_worker.load(Ordering::Acquire) => {
                        let (playback_started, _started_rx) = async_channel::bounded(1);
                        let (completed, completion_rx) = async_channel::bounded(1);
                        let duration = match sink.write_chunk_with_completion(
                            &pcm,
                            playback_started,
                            completed,
                        ) {
                            Ok(duration) => duration,
                            Err(error) => {
                                let _ = error_tx.try_send(error);
                                break;
                            }
                        };
                        playback_tail = playback_tail.max(Instant::now()) + duration;
                        events.push(TtsPlaybackEvent::PcmQueued(PcmEnergyTimeline::from_pcm(&pcm)));
                        if !started {
                            events.push(TtsPlaybackEvent::Started);
                            started = true;
                        }
                        pending.push(PendingCompletion {
                            receiver: completion_rx,
                            deadline: playback_tail + COMPLETION_GRACE,
                        });
                    }
                    Ok(_) => {},
                    Err(_) => break,
                },
                recv(completion_tick) -> _ => {
                    let mut callback_lost = false;
                    let mut callback_timed_out = false;
                    let now = Instant::now();
                    pending.retain(|completion| match completion.receiver.try_recv() {
                        Ok(()) => false,
                        Err(async_channel::TryRecvError::Empty) => {
                            if now >= completion.deadline {
                                callback_timed_out = true;
                                false
                            } else {
                                true
                            }
                        }
                        Err(async_channel::TryRecvError::Closed) => {
                            callback_lost = true;
                            false
                        }
                    });
                    if callback_lost {
                        let _ = error_tx.try_send("豆包音频播放完成回调丢失".to_string());
                        break;
                    }
                    if callback_timed_out {
                        let _ = error_tx.try_send("豆包音频播放完成回调超时".to_string());
                        break;
                    }
                },
            }
            if let Some((generation, done)) = finishing.take() {
                if generation == epoch_worker.load(Ordering::Acquire)
                    && audio_rx.is_empty()
                    && pending.is_empty()
                {
                    if started {
                        events.push(TtsPlaybackEvent::Finished);
                        started = false;
                    }
                    playback_tail = Instant::now();
                    let _ = done.try_send(());
                } else {
                    finishing = Some((generation, done));
                }
            }
        }
        sink.stop();
        if started {
            events.push(TtsPlaybackEvent::Interrupted);
        }
    });
    PlaybackQueue {
        audio: audio_tx,
        control: control_tx,
        epoch,
        errors: error_rx,
    }
}
