use asr::{
    InputEndReason, RecognitionEvent, RecognitionPipeline, RecognitionSession, RecognizerFactory,
};
use std::sync::Arc;
use std::thread::JoinHandle;
use voice_input::PcmStreamSession;

pub(crate) struct AsrRecordingSession {
    pcm_stream: Option<PcmStreamSession>,
    recognition: Arc<RecognitionSession>,
    forward_thread: Option<JoinHandle<()>>,
    event_thread: Option<JoinHandle<Result<String, String>>>,
}

impl AsrRecordingSession {
    pub(crate) fn reached_duration_limit(&self) -> bool {
        self.recognition.reached_duration_limit()
    }

    pub(crate) fn start(
        input_device_id: Option<String>,
        factory: &dyn RecognizerFactory,
        cancellation: asr::RecognitionCancellation,
    ) -> Result<(Self, async_channel::Receiver<String>), String> {
        let recognizer = factory
            .create_with_cancellation(cancellation.clone())
            .map_err(|error| error.to_string())?;
        if cancellation.is_cancelled() {
            return Err("recording startup cancelled".to_string());
        }
        let (pcm_stream, pcm_rx) =
            voice_input::start_pcm_stream(input_device_id).map_err(|error| error.to_string())?;
        if cancellation.is_cancelled() {
            pcm_stream.cancel();
            return Err("recording startup cancelled".to_string());
        }
        let recognition = Arc::new(
            RecognitionPipeline::new(factory.capabilities())
                .start_with_cancellation(recognizer, cancellation),
        );
        let forward_recognition = recognition.clone();
        let forward_thread = std::thread::Builder::new()
            .name("asr_pcm_forwarder".to_string())
            .spawn(move || {
                while let Ok(chunk) = pcm_rx.recv_blocking() {
                    if forward_recognition.push_pcm(chunk).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("could not spawn ASR PCM forwarder: {error}"))?;
        let events = recognition.events();
        let (partial_tx, partial_rx) = async_channel::bounded(64);
        let event_thread = std::thread::Builder::new()
            .name("asr_event_forwarder".to_string())
            .spawn(move || collect_terminal(events, partial_tx))
            .map_err(|error| format!("could not spawn ASR event forwarder: {error}"))?;
        Ok((
            Self {
                pcm_stream: Some(pcm_stream),
                recognition,
                forward_thread: Some(forward_thread),
                event_thread: Some(event_thread),
            },
            partial_rx,
        ))
    }

    pub(crate) fn finish(mut self) -> Result<String, String> {
        self.stop_capture();
        if let Err(error) = self.recognition.finish(InputEndReason::User) {
            if error != asr::AsrError::SessionClosed {
                return Err(error.to_string());
            }
        }
        self.join_terminal()
    }

    pub(crate) fn cancel(mut self) {
        self.recognition.cancellation().cancel();
        let _ = std::thread::Builder::new()
            .name("asr_recording_cancel".to_string())
            .spawn(move || {
                let _ = self.recognition.cancel();
                self.stop_capture();
                let _ = self.join_terminal();
            });
    }

    fn stop_capture(&mut self) {
        if let Some(pcm_stream) = self.pcm_stream.take() {
            pcm_stream.cancel();
        }
        if let Some(thread) = self.forward_thread.take() {
            let _ = thread.join();
        }
    }

    fn join_terminal(&mut self) -> Result<String, String> {
        self.event_thread
            .take()
            .ok_or_else(|| "ASR event forwarder is missing".to_string())?
            .join()
            .map_err(|_| "ASR event forwarder panicked".to_string())?
    }
}

impl Drop for AsrRecordingSession {
    fn drop(&mut self) {
        self.recognition.cancellation().cancel();
    }
}

fn collect_terminal(
    events: async_channel::Receiver<RecognitionEvent>,
    partials: async_channel::Sender<String>,
) -> Result<String, String> {
    while let Ok(event) = events.recv_blocking() {
        match event {
            RecognitionEvent::Partial(text) => {
                let _ = partials.try_send(text);
            }
            RecognitionEvent::Final(text) => return Ok(text),
            RecognitionEvent::Empty => return Ok(String::new()),
            RecognitionEvent::Failed(error) => return Err(error.to_string()),
            RecognitionEvent::Cancelled => return Err("recognition was cancelled".to_string()),
            RecognitionEvent::InputEnded(_) => {}
        }
    }
    Err("recognition ended without a terminal event".to_string())
}
