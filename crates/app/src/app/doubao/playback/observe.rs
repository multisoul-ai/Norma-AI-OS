use super::PendingCompletion;
use crate::app::shared_context::report::ReportDeliveryHandle;
use crate::app::voice_window_inset::{TtsPlaybackEvent, VoiceWindowInsetEventSink};
use agent::context::ReportDelivery;

pub(super) fn observe_start(
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
