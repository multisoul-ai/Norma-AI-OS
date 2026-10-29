//! Legacy TTS shares the audio worker's start, cancellation and deadline semantics.
use super::playback::{AgentAnnouncementAudioControl, AgentAnnouncementPlaybackPhase};
use super::AgentAnnouncement;
use crate::app::doubao::playback::start_announcement_playback_with;
use crate::app::voice::playback::AssistantAudioSink;
use crate::app::voice_window_inset::{
    TtsPlaybackEvent, TtsPlaybackSource, VoiceWindowInsetEventBus,
};
use crate::app::MultisoulApp;
use futures::{select_biased, FutureExt};
use gpui::Context;
use std::sync::Arc;

impl MultisoulApp {
    pub(super) fn start_legacy_announcement_playback(
        &mut self,
        generation: u64,
        announcement: AgentAnnouncement,
        pcm: audio::PcmAudioChunk,
        make_sink: impl FnOnce() -> Box<dyn AssistantAudioSink> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let (events, wake) = VoiceWindowInsetEventBus::new();
        let queue = Arc::new(start_announcement_playback_with(
            make_sink,
            events.sink(TtsPlaybackSource::AgentAnnouncement),
        ));
        let scheduled = (|| {
            if let Some(delivery) = &announcement.delivery {
                queue.track_report(0, delivery.clone())?;
            }
            queue.enqueue_for(0, pcm)?;
            queue.finish_for(0)
        })();
        let done = match scheduled {
            Ok(done) => done,
            Err(error) => {
                if let Some(delivery) = &announcement.delivery {
                    delivery.set(agent::context::ReportDelivery::Failed);
                }
                queue.end();
                tracing::warn!(?error, "agent announcement scheduling failed");
                // Use the same stop-confirmation and queue continuation path even
                // when the worker fails before finish_for can be submitted.
                let (sender, done) = async_channel::bounded(1);
                drop(sender);
                done
            }
        };
        self.agent_announcement_playback.control = Some(AgentAnnouncementAudioControl {
            queue: Arc::clone(&queue),
        });
        self.agent_announcement_playback.phase = AgentAnnouncementPlaybackPhase::Playing {
            announcement,
            started: false,
        };
        // This task owns only the monitor handle, never the foreground cancellation owner.
        cx.spawn(async move |this, cx| {
            loop {
                let completed = select_biased! {
                    result = done.recv().fuse() => Some(result.is_ok()),
                    error = queue.errors.recv().fuse() => {
                        tracing::warn!(?error, "agent announcement audio worker failed");
                        Some(false)
                    },
                    _ = wake.recv().fuse() => None,
                };
                // A completion signal may win over wake; drain Started before ending UI playback.
                let current = this
                    .update(cx, |app, cx| {
                        if app.agent_announcement_playback.generation != generation {
                            return false;
                        }
                        for (_, event) in events.drain() {
                            match event {
                                TtsPlaybackEvent::Started => {
                                    app.mark_agent_announcement_started(generation);
                                }
                                TtsPlaybackEvent::PcmQueued(_) => {
                                    app.agent_announcement_playback.push_playback_event(event)
                                }
                                _ => {}
                            }
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if completed.is_some() || !current {
                    queue.end();
                    queue.wait_stopped().await;
                    let _ = this.update(cx, |app, cx| {
                        if current {
                            app.finish_agent_announcement_playback(
                                generation,
                                completed.unwrap_or(false),
                                cx,
                            );
                        }
                        // Cancellation can have queued the next report while hardware was stopping.
                        app.start_next_agent_announcement(cx);
                    });
                    break;
                }
            }
        })
        .detach();
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/legacy_report_playback_tests.rs"]
mod legacy_report_tests;
