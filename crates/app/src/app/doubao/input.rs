use async_channel::{Receiver, Sender};
use audio::PcmAudioChunk;
use tokio_util::sync::CancellationToken;
use ui::i18n::LocalizedText;

pub(super) type RecordingInput = (u64, Receiver<Result<PcmAudioChunk, LocalizedText>>);

pub(super) struct SessionInput {
    pub events: Sender<super::plan::asr::SpeechEvent>,
    pub speech: Receiver<super::plan::asr::SpeechEvent>,
    pub recordings: Option<Receiver<RecordingInput>>,
}

impl crate::app::MultisoulApp {
    pub(crate) fn begin_doubao_recording(
        &mut self,
        from_shortcut: bool,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.doubao.recording.is_some() {
            return;
        }
        if !self.doubao.is_active() {
            self.toggle_doubao_voice(window, cx);
        }
        let (Some(recordings), Some(cancellation)) =
            (&self.doubao.recordings, &self.doubao.cancellation)
        else {
            return;
        };
        let Ok(runtime) = crate::app::shared_tokio_runtime() else {
            return;
        };
        self.doubao.recording_id = self.doubao.recording_id.wrapping_add(1);
        let recording_id = self.doubao.recording_id;
        if let Some(speech) = &self.doubao.speech {
            let _ = speech.try_send(super::plan::asr::SpeechEvent::HeldStarted(recording_id));
        }
        let finish = CancellationToken::new();
        let input = record(
            runtime,
            finish.clone(),
            cancellation.clone(),
            self.voice_settings
                .values()
                .input_device_id()
                .map(str::to_owned),
        );
        if recordings.try_send((recording_id, input)).is_ok() {
            self.doubao.recording = Some(finish);
            self.doubao.recording_from_shortcut = from_shortcut;
            self.doubao.awaiting_transcript = false;
            self.doubao.status = super::DoubaoStatus::Connected;
        } else {
            finish.cancel();
        }
        cx.notify();
    }

    pub(crate) fn end_doubao_recording(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(recording) = self.doubao.recording.take() {
            self.doubao.recording_from_shortcut = false;
            self.doubao.awaiting_transcript = true;
            recording.cancel();
            self.doubao.status = super::DoubaoStatus::Thinking;
            cx.notify();
        }
    }
}

pub(super) fn record(
    runtime: &tokio::runtime::Runtime,
    finish: CancellationToken,
    cancellation: CancellationToken,
    device: Option<String>,
) -> Receiver<Result<PcmAudioChunk, LocalizedText>> {
    let (sender, input) = async_channel::unbounded();
    runtime.spawn(async move {
        if let Err(error) = capture(&sender, finish, cancellation, device).await {
            let _ = sender.send(Err(error)).await;
        }
    });
    input
}

async fn capture(
    sender: &Sender<Result<PcmAudioChunk, LocalizedText>>,
    finish: CancellationToken,
    cancellation: CancellationToken,
    device: Option<String>,
) -> Result<(), LocalizedText> {
    if finish.is_cancelled() || cancellation.is_cancelled() {
        return Ok(());
    }
    let (capture, chunks) =
        tokio::task::spawn_blocking(move || voice_input::start_pcm_stream(device))
            .await
            .map_err(|_| LocalizedText::new("doubao.microphone_start"))?
            .map_err(|error| {
                LocalizedText::new("doubao.microphone_start_reason").with_arg("reason", error)
            })?;
    // ponytail: 缓冲最多五分钟 PCM（16,000 × 2 × 300 = 9.6 MB），更长录音再改为落盘。
    let mut bytes = 0;
    let result = loop {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => break Ok(()),
            _ = finish.cancelled() => break Ok(()),
            chunk = chunks.recv() => {
                let Ok(chunk) = chunk else { break Err(LocalizedText::new("doubao.microphone_stopped")); };
                bytes += chunk.bytes.len();
                if bytes > 16_000 * 2 * 300 { break Err(LocalizedText::new("doubao.recording_too_long")); }
                if sender.send(Ok(chunk)).await.is_err() { break Ok(()); }
            }
        }
    };
    // 停止及 join 必须后台执行；松键后仍保留停止前已采集的尾音。
    let _ = tokio::task::spawn_blocking(move || drop(capture)).await;
    if !cancellation.is_cancelled() && result.is_ok() {
        while let Ok(chunk) = chunks.try_recv() {
            if sender.send(Ok(chunk)).await.is_err() {
                break;
            }
        }
    }
    result
}
