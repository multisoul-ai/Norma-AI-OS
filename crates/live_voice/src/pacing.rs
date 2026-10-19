use crate::LiveError;
use audio::PcmAudioChunk;
use std::time::Duration;

#[derive(Default)]
pub struct AudioPacer {
    next: Duration,
}

impl AudioPacer {
    pub fn reserve(&mut self, now: Duration, pcm: &PcmAudioChunk) -> Result<Duration, LiveError> {
        super::session::validate_audio(pcm)?;
        let start = self.next.max(now);
        // 16k mono PCM16 has 32000 bytes/s; the one-second input bound prevents overflow.
        self.next = start + Duration::from_nanos(pcm.bytes.len() as u64 * 1_000_000_000 / 32_000);
        Ok(start)
    }
}
