use crate::app::doubao::playback::PlaybackQueue;
use std::time::Duration;

// Live has no server-side clear, so a short backlog stays for natural backchannels.
const STALE_BACKLOG: Duration = Duration::from_secs(1);
// Sustained speech, not a short "嗯", cough or echo blip.
const SUSTAINED_SPEECH: Duration = Duration::from_millis(600);

/// Barge-in drops local audio the user would otherwise hear long after they started talking.
#[derive(Default)]
pub(super) struct BargeIn {
    speaking: Duration,
    handled: bool,
}

impl BargeIn {
    /// Feed each uploaded PCM span with its VAD state; returns the dropped backlog, at most once per utterance.
    pub(super) fn observe(
        &mut self,
        speech_active: bool,
        pcm: &audio::PcmAudioChunk,
        playback: &PlaybackQueue,
    ) -> Option<Duration> {
        if !speech_active {
            *self = Self::default();
            return None;
        }
        self.speaking += crate::app::voice::playback::pcm_chunk_duration(pcm);
        if self.handled || self.speaking < SUSTAINED_SPEECH {
            return None;
        }
        self.handled = true;
        let buffered = playback.buffered_duration();
        (buffered > STALE_BACKLOG).then(|| {
            playback.interrupt();
            buffered
        })
    }
}

pub(super) fn set_blocked(
    blocked: bool,
    playback: &crate::app::doubao::playback::PlaybackQueue,
    control: &live_voice::LiveControl,
    events: &async_channel::Sender<super::session::CallEvent>,
) -> Result<(), String> {
    control.set_output_blocked(blocked);
    playback.interrupt();
    control.send_command(&live_voice::LiveCommand::Append {
        event_id: uuid::Uuid::new_v4().to_string(),
        kind: live_voice::AppendKind::Instructions,
        delegation_id: None,
        content: if blocked {
            "宿主已停止本地播报并丢弃已接收的待播音频；麦克风继续工作，已接受任务继续执行。不要声称用户听到了被丢弃的语音。"
        } else {
            "宿主已恢复接收新语音；本地停止期间已接收音频已丢弃，不会补播，已接受任务继续执行。"
        }.into(),
    }).map_err(|error|error.to_string())?;
    events
        .try_send(super::session::CallEvent::OutputBlocked(blocked))
        .map_err(|_| "Live UI event queue is full or closed".into())
}

#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_barge_in_tests.rs"]
mod barge_in_tests;
