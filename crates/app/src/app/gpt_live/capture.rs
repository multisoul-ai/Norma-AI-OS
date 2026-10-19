use super::session::{CaptureEvent, CaptureMode};
use tokio_util::sync::CancellationToken;

pub(super) async fn run_capture(
    input_device: Option<String>,
    mut desired: tokio::sync::watch::Receiver<CaptureMode>,
    events: async_channel::Sender<CaptureEvent>,
    cancellation: CancellationToken,
) {
    loop {
        let mode = *desired.borrow_and_update();
        if cancellation.is_cancelled() {
            return;
        }
        if mode.muted {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return,
                sent = events.send(CaptureEvent::Stopped { generation: mode.generation }) => if sent.is_err() { return; },
            }
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return,
                changed = desired.changed() => if changed.is_err() { return; },
            }
            continue;
        }
        let device = input_device.clone();
        let started =
            tokio::task::spawn_blocking(move || voice_input::start_duplex_pcm_stream(device)).await;
        let (capture, incoming) = match started {
            Ok(Ok(capture)) => capture,
            result => {
                let message = match result {
                    Ok(Err(error)) => error.to_string(),
                    Err(error) => error.to_string(),
                    _ => unreachable!(),
                };
                if *desired.borrow() != mode {
                    continue;
                }
                tokio::select! {
                    biased;
                    _ = cancellation.cancelled() => return,
                    changed = desired.changed() => if changed.is_err() { return; } else { continue; },
                    _ = events.send(CaptureEvent::Failed { generation:mode.generation, message }) => return,
                }
            }
        };
        if cancellation.is_cancelled() || *desired.borrow() != mode {
            stop_capture(capture).await;
            if !cancellation.is_cancelled() {
                forward_tail(&incoming, mode.generation, &events, &cancellation).await;
            }
            continue;
        }
        let announced = tokio::select! {
            biased;
            _ = cancellation.cancelled() => false,
            _ = desired.changed() => false,
            sent = events.send(CaptureEvent::Ready { generation:mode.generation }) => sent.is_ok(),
        };
        if !announced {
            stop_capture(capture).await;
            if !cancellation.is_cancelled() {
                forward_tail(&incoming, mode.generation, &events, &cancellation).await;
            }
            if !cancellation.is_cancelled() && *desired.borrow() != mode {
                continue;
            }
            return;
        }
        let mut failure = None;
        let stop_all = loop {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => break true,
                changed = desired.changed() => break changed.is_err(),
                next = incoming.recv() => match next {
                    Ok(chunk) => if events.try_send(CaptureEvent::Pcm { generation:mode.generation, chunk }).is_err() {
                        failure = Some("Microphone event queue is full or closed".into());
                        break true;
                    },
                    Err(_) => { failure = Some("Microphone capture ended unexpectedly".into()); break true; },
                },
            }
        };
        // The old stream owns its generation. Join it before opening another microphone stream.
        stop_capture(capture).await;
        if !cancellation.is_cancelled() {
            forward_tail(&incoming, mode.generation, &events, &cancellation).await;
        }
        if let Some(message) = failure {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => {},
                _ = events.send(CaptureEvent::Failed { generation:mode.generation, message }) => {},
            }
        }
        if stop_all {
            return;
        }
    }
}

async fn forward_tail(
    incoming: &async_channel::Receiver<audio::PcmAudioChunk>,
    generation: u64,
    events: &async_channel::Sender<CaptureEvent>,
    cancellation: &CancellationToken,
) {
    while let Ok(chunk) = incoming.try_recv() {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return,
            sent = events.send(CaptureEvent::Pcm { generation, chunk }) => if sent.is_err() { return; },
        }
    }
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => {},
        _ = events.send(CaptureEvent::Stopped { generation }) => {},
    }
}

async fn stop_capture(capture: voice_input::PcmStreamSession) {
    let _ = tokio::task::spawn_blocking(move || capture.cancel()).await;
}

#[cfg(all(test, feature = "e2e-test-support"))]
#[path = "../../../tests/unit/gpt_live_capture_tests.rs"]
mod tests;
