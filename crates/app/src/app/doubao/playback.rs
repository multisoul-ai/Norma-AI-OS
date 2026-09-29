use audio::PcmAudioChunk;
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

    pub(super) fn interrupt(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let _ = self.control.send(false);
    }

    pub(super) fn end(&self) {
        self.interrupt();
        let _ = self.control.send(true);
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
