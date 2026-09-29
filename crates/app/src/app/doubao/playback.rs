use super::*;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

pub(super) struct PlaybackQueue {
    audio: crossbeam_channel::Sender<(u64, PcmAudioChunk)>,
    control: crossbeam_channel::Sender<bool>,
    epoch: Arc<AtomicU64>,
    pub(super) errors: async_channel::Receiver<String>,
}

impl PlaybackQueue {
    pub(super) fn enqueue(&self, pcm: PcmAudioChunk) -> Result<(), String> {
        let generation = self.epoch.load(Ordering::Acquire);
        self.audio
            .try_send((generation, pcm))
            .map_err(|_| "豆包音频播放跟不上实时语速，请检查输出设备".to_string())
    }

    pub(super) fn interrupt(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let _ = self.control.send(false);
    }

    pub(super) fn end(&self) {
        self.interrupt();
        let _ = self.control.send(true);
    }
}

#[derive(Default)]
pub(super) struct AudioTurnGate {
    pub(super) active: Option<String>,
    blocked: HashSet<String>,
    pub(super) waiting_for_next_start: bool,
}

impl AudioTurnGate {
    pub(super) fn interrupt(&mut self) {
        if let Some(id) = self.active.take() {
            self.blocked.insert(id);
        }
        self.waiting_for_next_start = true;
    }

    pub(super) fn started(&mut self, id: Option<&str>, user_speaking: bool) -> bool {
        if user_speaking || id.is_some_and(|id| self.blocked.contains(id)) {
            return false;
        }
        self.waiting_for_next_start = false;
        self.active = id.filter(|id| !id.is_empty()).map(str::to_owned);
        true
    }

    pub(super) fn accept_delta(&mut self, id: Option<&str>, user_speaking: bool) -> bool {
        let Some(id) = id.filter(|id| !id.is_empty()) else {
            return false;
        };
        if user_speaking || self.waiting_for_next_start || self.blocked.contains(id) {
            return false;
        }
        if self.active.as_deref().is_some_and(|active| active != id) {
            return false;
        }
        self.active = Some(id.to_owned());
        true
    }

    pub(super) fn finished(&mut self, id: Option<&str>) -> bool {
        if self.active.as_deref() != id {
            return false;
        }
        self.active = None;
        true
    }
}

pub(super) fn start_playback() -> PlaybackQueue {
    start_playback_with(crate::app::voice::playback::default_assistant_audio_sink)
}

pub(super) fn start_playback_with(
    make_sink: impl FnOnce() -> Box<dyn crate::app::voice::playback::AssistantAudioSink>
        + Send
        + 'static,
) -> PlaybackQueue {
    let (audio_tx, audio_rx) = crossbeam_channel::bounded(16);
    let (control_tx, control_rx) = crossbeam_channel::unbounded();
    let (error_tx, error_rx) = async_channel::bounded(1);
    let epoch = Arc::new(AtomicU64::new(0));
    let epoch_worker = Arc::clone(&epoch);
    std::thread::spawn(move || {
        let mut sink = make_sink();
        loop {
            if let Ok(end) = control_rx.try_recv() {
                sink.stop();
                if end {
                    break;
                }
                continue;
            }
            crossbeam_channel::select! {
                recv(control_rx) -> command => {
                    sink.stop();
                    if command.is_err() || command == Ok(true) { break; }
                }
                recv(audio_rx) -> command => match command {
                    Ok((generation, pcm)) if generation == epoch_worker.load(Ordering::Acquire) => {
                        if let Err(error) = sink.write_chunk(&pcm) {
                            let _ = error_tx.try_send(error);
                            break;
                        }
                    }
                    Ok(_) => {},
                    Err(_) => break,
                }
            }
        }
        sink.stop();
    });
    PlaybackQueue {
        audio: audio_tx,
        control: control_tx,
        epoch,
        errors: error_rx,
    }
}
