use audio::PcmAudioChunk;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use ui::i18n::LocalizedText;

// Live PCM may arrive in 1s packets. Count both queued and hardware-scheduled PCM.
const LIMIT_NANOS: u64 = 1_000_000_000;

#[derive(Default)]
pub(super) struct Budget(AtomicU64);

impl Budget {
    pub(super) fn buffered(&self) -> Duration {
        Duration::from_nanos(self.0.load(Ordering::Acquire))
    }

    pub(super) fn reserve(
        self: &Arc<Self>,
        pcm: &PcmAudioChunk,
    ) -> Result<Reservation, LocalizedText> {
        if pcm.channels != 1
            || !(8000..=192000).contains(&pcm.sample_rate_hz)
            || !pcm.bytes.len().is_multiple_of(2)
        {
            return Err(
                LocalizedText::new("doubao.playback_failed").with_arg("reason", "Invalid PCM")
            );
        }
        if pcm.bytes.len() > pcm.sample_rate_hz as usize * 2 {
            return Err(LocalizedText::new("doubao.playback_overflow"));
        }
        let nanos =
            (pcm.bytes.len() as u64 / 2 * 1_000_000_000).div_ceil(u64::from(pcm.sample_rate_hz));
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |held| {
                held.checked_add(nanos)
                    .filter(|total| *total <= LIMIT_NANOS)
            })
            .map_err(|_| LocalizedText::new("doubao.playback_overflow"))?;
        Ok(Reservation {
            budget: self.clone(),
            nanos,
        })
    }
}

pub(super) struct Reservation {
    budget: Arc<Budget>,
    nanos: u64,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.0.fetch_sub(self.nanos, Ordering::AcqRel);
    }
}
