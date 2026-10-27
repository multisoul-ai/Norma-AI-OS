use crate::app::shared_context::report::ReportDeliveryHandle;
use crate::app::voice_window_inset::{
    PcmEnergyTimeline, TtsPlaybackEvent, VoiceWindowInsetEventSink,
};
use agent::context::ReportDelivery;
use audio::PcmAudioChunk;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use ui::i18n::LocalizedText;

// TTS admits at most 30 s of PCM; 100 ms coalescing produces at most 301 buffers.
const MAX_PENDING_COMPLETIONS: usize = 512;
const COMPLETION_GRACE: Duration = Duration::from_secs(5);

struct BudgetedAudio {
    generation: u64,
    pcm: PcmAudioChunk,
    reservation: Option<budget::Reservation>,
}

mod budget;

struct PendingCompletion {
    generation: u64,
    started: async_channel::Receiver<()>,
    _reservation: Option<budget::Reservation>,
    receiver: async_channel::Receiver<()>,
    deadline: Instant,
}

enum PlaybackControl {
    TrackReport {
        generation: u64,
        delivery: ReportDeliveryHandle,
    },
    Interrupt,
    End,
    Finish {
        generation: u64,
        done: async_channel::Sender<()>,
    },
}

pub(in crate::app) struct PlaybackQueue {
    audio: crossbeam_channel::Sender<BudgetedAudio>,
    control: crossbeam_channel::Sender<PlaybackControl>,
    epoch: Arc<AtomicU64>,
    stopped: async_channel::Receiver<()>,
    budget: Option<Arc<budget::Budget>>,
    pub(in crate::app) errors: async_channel::Receiver<LocalizedText>,
}

impl PlaybackQueue {
    pub(in crate::app) fn track_report(
        &self,
        generation: u64,
        delivery: ReportDeliveryHandle,
    ) -> Result<(), LocalizedText> {
        self.control
            .send(PlaybackControl::TrackReport {
                generation,
                delivery,
            })
            .map_err(|_| LocalizedText::new("doubao.playback_ended"))
    }
    pub(in crate::app) fn buffered_duration(&self) -> Duration {
        self.budget
            .as_ref()
            .map_or(Duration::ZERO, |budget| budget.buffered())
    }

    pub(in crate::app) fn is_stopped(&self) -> bool {
        self.stopped.is_closed()
    }

    pub(in crate::app) async fn wait_stopped(&self) {
        let _ = self.stopped.recv().await;
    }

    pub(in crate::app) fn generation(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub(in crate::app) fn enqueue_for(
        &self,
        generation: u64,
        pcm: PcmAudioChunk,
    ) -> Result<(), LocalizedText> {
        if generation != self.generation() {
            return Ok(());
        }
        let reservation = self
            .budget
            .as_ref()
            .map(|budget| budget.reserve(&pcm))
            .transpose()?;
        self.audio
            .try_send(BudgetedAudio {
                generation,
                pcm,
                reservation,
            })
            .map_err(|_| LocalizedText::new("doubao.playback_overflow"))
    }

    pub(in crate::app) fn finish_for(
        &self,
        generation: u64,
    ) -> Result<async_channel::Receiver<()>, LocalizedText> {
        let (done, receiver) = async_channel::bounded(1);
        self.control
            .send(PlaybackControl::Finish { generation, done })
            .map_err(|_| LocalizedText::new("doubao.playback_ended"))?;
        Ok(receiver)
    }

    pub(in crate::app) fn interrupt(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let _ = self.control.send(PlaybackControl::Interrupt);
    }

    pub(in crate::app) fn end(&self) {
        self.interrupt();
        let _ = self.control.send(PlaybackControl::End);
    }
}

pub(in crate::app) fn start_playback(events: VoiceWindowInsetEventSink) -> PlaybackQueue {
    start_playback_with(
        crate::app::voice::playback::default_assistant_audio_sink,
        events,
    )
}

pub(in crate::app) fn start_playback_with(
    make_sink: impl FnOnce() -> Box<dyn crate::app::voice::playback::AssistantAudioSink>
        + Send
        + 'static,
    events: VoiceWindowInsetEventSink,
) -> PlaybackQueue {
    start_playback_worker(make_sink, events, None, COMPLETION_GRACE)
}

pub(in crate::app) fn start_live_playback(events: VoiceWindowInsetEventSink) -> PlaybackQueue {
    start_live_playback_with(
        crate::app::voice::playback::default_assistant_audio_sink,
        events,
    )
}

pub(in crate::app) fn start_live_playback_with(
    make_sink: impl FnOnce() -> Box<dyn crate::app::voice::playback::AssistantAudioSink>
        + Send
        + 'static,
    events: VoiceWindowInsetEventSink,
) -> PlaybackQueue {
    start_playback_worker(
        make_sink,
        events,
        Some(Arc::new(budget::Budget::default())),
        COMPLETION_GRACE,
    )
}

/// Legacy keeps its original PCM duration + 2 s callback deadline.
pub(in crate::app) fn start_announcement_playback_with(
    make_sink: impl FnOnce() -> Box<dyn crate::app::voice::playback::AssistantAudioSink>
        + Send
        + 'static,
    events: VoiceWindowInsetEventSink,
) -> PlaybackQueue {
    start_playback_worker(make_sink, events, None, Duration::from_secs(2))
}

fn start_playback_worker(
    make_sink: impl FnOnce() -> Box<dyn crate::app::voice::playback::AssistantAudioSink>
        + Send
        + 'static,
    events: VoiceWindowInsetEventSink,
    budget: Option<Arc<budget::Budget>>,
    completion_grace: Duration,
) -> PlaybackQueue {
    let (audio_tx, audio_rx) = crossbeam_channel::bounded(16);
    let (control_tx, control_rx) = crossbeam_channel::unbounded();
    let (error_tx, error_rx) = async_channel::bounded(1);
    let (stopped_tx, stopped) = async_channel::bounded::<()>(1);
    let epoch = Arc::new(AtomicU64::new(0));
    let epoch_worker = Arc::clone(&epoch);
    std::thread::spawn(move || {
        let mut sink = make_sink();
        let tick = crossbeam_channel::tick(Duration::from_millis(12));
        let idle = crossbeam_channel::never::<Instant>();
        let paused_audio = crossbeam_channel::never::<BudgetedAudio>();
        let mut pending = Vec::<PendingCompletion>::new();
        let mut finishing: Option<(u64, async_channel::Sender<()>)> = None;
        let mut started = false;
        let mut delivery: Option<(u64, ReportDeliveryHandle)> = None;
        let mut failed = false;
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
                    Ok(PlaybackControl::TrackReport { generation, delivery: incoming }) => {
                        if generation == epoch_worker.load(Ordering::Acquire) {
                            if let Some((_, old)) = delivery.replace((generation, incoming)) { old.cancel(); }
                        } else { incoming.cancel(); }
                    }
                    Ok(PlaybackControl::Interrupt) => {
                        if let Err(error) = sink.stop_and_confirm() {
                            failed = true;
                            let _ = error_tx.try_send(LocalizedText::new("doubao.playback_failed").with_arg("reason", error));
                            break;
                        }
                        // The epoch was retired before this command arrived. Hardware
                        // may already have started; drain its evidence after stop confirms.
                        if let Some(completion) = pending.first() {
                            observe_start(&pending, &mut started, &events, &delivery, completion.generation);
                        }
                        if let Some((_, delivery)) = delivery.take() { delivery.cancel(); }
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
                    Ok(BudgetedAudio { generation, pcm, reservation }) if generation == epoch_worker.load(Ordering::Acquire) => {
                        let (playback_started, started_rx) = async_channel::bounded(1);
                        let (completed, completion_rx) = async_channel::bounded(1);
                        pending.push(PendingCompletion {
                            generation,
                            started: started_rx,
                            _reservation: reservation,
                            receiver: completion_rx,
                            deadline: playback_tail.max(Instant::now()) + crate::app::voice::playback::pcm_chunk_duration(&pcm) + completion_grace,
                        });
                        let duration = match sink.write_chunk_with_completion(
                            &pcm,
                            playback_started,
                            completed,
                        ) {
                            Ok(duration) => duration,
                            Err(error) => {
                                failed = true;
                                let _ = error_tx.try_send(LocalizedText::new("doubao.playback_failed").with_arg("reason", error));
                                break;
                            }
                        };
                        playback_tail = playback_tail.max(Instant::now()) + duration;
                        events.push(TtsPlaybackEvent::PcmQueued(PcmEnergyTimeline::from_pcm(&pcm)));
                        if let Some(pending) = pending.last_mut() {
                            pending.deadline = playback_tail + completion_grace;
                        }
                    }
                    Ok(_) => {},
                    Err(_) => break,
                },
                recv(completion_tick) -> _ => {
                    observe_start(&pending, &mut started, &events, &delivery, epoch_worker.load(Ordering::Acquire));
                    let mut callback_lost = false;
                    let mut callback_timed_out = false;
                    let now = Instant::now();
                    pending.retain(|completion| match completion.receiver.try_recv() {
                        Ok(()) => {
                            // Started and completed can arrive between the first observation
                            // and this receive. Do not drop the only start evidence.
                            observe_start(std::slice::from_ref(completion), &mut started, &events, &delivery, completion.generation);
                            false
                        },
                        Err(async_channel::TryRecvError::Empty) => {
                            if now >= completion.deadline {
                                callback_timed_out = true;
                                true
                            } else {
                                true
                            }
                        }
                        Err(async_channel::TryRecvError::Closed) => {
                            callback_lost = true;
                            true
                        }
                    });
                    if callback_lost {
                        failed = true;
                        let _ = error_tx.try_send(LocalizedText::new("doubao.playback_callback_lost"));
                        break;
                    }
                    if callback_timed_out {
                        failed = true;
                        let _ = error_tx.try_send(LocalizedText::new("doubao.playback_callback_timeout"));
                        break;
                    }
                },
            }
            observe_start(
                &pending,
                &mut started,
                &events,
                &delivery,
                epoch_worker.load(Ordering::Acquire),
            );
            if let Some((generation, done)) = finishing.take() {
                if generation == epoch_worker.load(Ordering::Acquire)
                    && audio_rx.is_empty()
                    && pending.is_empty()
                {
                    if !started {
                        failed = true;
                        let _ =
                            error_tx.try_send(LocalizedText::new("doubao.playback_callback_lost"));
                        break;
                    }
                    if let Some((_, delivery)) = delivery.take() {
                        delivery.set(ReportDelivery::Completed);
                    }
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
        if sink.stop_and_confirm().is_err() {
            failed = true;
        }
        if let Some(completion) = pending.first() {
            observe_start(
                &pending,
                &mut started,
                &events,
                &delivery,
                completion.generation,
            );
        }
        if let Some((_, delivery)) = delivery.take() {
            if failed {
                delivery.set(ReportDelivery::Failed);
            } else {
                delivery.cancel();
            }
        }
        if started {
            events.push(TtsPlaybackEvent::Interrupted);
        }
        drop(sink);
        pending.clear();
        drop(audio_rx);
        stopped_tx.close();
    });
    PlaybackQueue {
        audio: audio_tx,
        control: control_tx,
        epoch,
        errors: error_rx,
        stopped,
        budget,
    }
}

fn observe_start(
    pending: &[PendingCompletion],
    started: &mut bool,
    events: &VoiceWindowInsetEventSink,
    delivery: &Option<(u64, ReportDeliveryHandle)>,
    generation: u64,
) {
    if !*started
        && pending
            .iter()
            .any(|item| item.generation == generation && item.started.try_recv().is_ok())
    {
        *started = true;
        if let Some((_, delivery)) = delivery.as_ref().filter(|(bound, _)| *bound == generation) {
            delivery.set(ReportDelivery::Playing);
        }
        events.push(TtsPlaybackEvent::Started);
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_playback_tests.rs"]
mod live_tests;

#[cfg(test)]
#[path = "../../../tests/unit/doubao_report_delivery_tests.rs"]
mod report_delivery_tests;
