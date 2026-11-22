use crate::app::voice::playback::{
    default_assistant_audio_sink, pcm_chunk_duration, AssistantAudioSink,
};
use audio::PcmAudioChunk;
use std::time::Duration;
use tokio::sync::watch;
use voice_input::DuplexPlayback;

/// Plays GPT-Live audio on the microphone's voice-processing engine.
///
/// A second engine exists only while that engine is fully released (mic muted).
/// Starting or stopping the call never runs both at once.
pub(super) struct LiveEngineSink {
    route: watch::Receiver<Option<DuplexPlayback>>,
    seen_engine: bool,
    fallback: Option<Box<dyn AssistantAudioSink>>,
}

impl LiveEngineSink {
    pub(super) fn new(route: watch::Receiver<Option<DuplexPlayback>>) -> Self {
        Self {
            route,
            seen_engine: false,
            fallback: None,
        }
    }

    fn stop_fallback(&mut self) {
        if let Some(mut fallback) = self.fallback.take() {
            fallback.stop();
        }
    }
}

impl AssistantAudioSink for LiveEngineSink {
    fn write_chunk(&mut self, chunk: &PcmAudioChunk) -> Result<Duration, String> {
        let (started, _) = async_channel::bounded(1);
        let (done, _) = async_channel::bounded(1);
        self.write_chunk_with_completion(chunk, started, done)
    }

    fn write_chunk_with_completion(
        &mut self,
        chunk: &PcmAudioChunk,
        started: async_channel::Sender<()>,
        done: async_channel::Sender<()>,
    ) -> Result<Duration, String> {
        let player = self.route.borrow().clone();
        if let Some(player) = player {
            self.stop_fallback();
            self.seen_engine = true;
            return match player.play_with_completion(chunk, started.clone(), done.clone()) {
                Ok(duration) => Ok(duration),
                Err(error) => {
                    tracing::warn!(
                        target: "norma_voice",
                        %error,
                        "GPT-Live output engine is stopping; dropping this chunk"
                    );
                    let _ = started.try_send(());
                    let _ = done.try_send(());
                    Ok(pcm_chunk_duration(chunk))
                }
            };
        }
        if !self.seen_engine {
            let _ = started.try_send(());
            let _ = done.try_send(());
            return Ok(pcm_chunk_duration(chunk));
        }
        self.fallback
            .get_or_insert_with(default_assistant_audio_sink)
            .write_chunk_with_completion(chunk, started, done)
    }

    fn stop(&mut self) {
        let player = self.route.borrow().clone();
        if let Some(player) = player {
            let _ = player.stop();
        }
        self.stop_fallback();
    }

    fn stop_and_confirm(&mut self) -> Result<(), String> {
        let player = self.route.borrow().clone();
        if let Some(player) = player {
            player.stop()?;
        }
        if let Some(fallback) = self.fallback.as_mut() {
            fallback.stop_and_confirm()?;
        }
        Ok(())
    }
}
