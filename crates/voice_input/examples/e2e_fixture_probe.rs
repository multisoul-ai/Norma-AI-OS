//! Diagnostic for the debug E2E audio fixture (`NORMA_E2E_AUDIO_FIXTURE`).
//!
//! Usage: `e2e_fixture_probe <silero_vad_dir> <seconds|status-only>`; `status-only` prints the
//! permission status and exits without starting capture (starting without a fixture would
//! raise the real microphone prompt). Prints the microphone permission
//! status the app would see, then runs the fixture stream through the same
//! `start_duplex_pcm_stream` entry point the app uses and through Silero VAD, so a
//! run shows whether audio reached a processing stage without opening the microphone.
use std::path::PathBuf;
use std::time::{Duration, Instant};
use voice_input::{microphone_authorization_status, start_duplex_pcm_stream, SherpaVadClient};

fn main() {
    let mut args = std::env::args().skip(1);
    let vad_dir = PathBuf::from(args.next().expect("silero vad dir"));
    let mode = args.next().unwrap_or_default();
    let seconds: u64 = mode.parse().unwrap_or(6);
    println!(
        "fixture_env={:?}",
        std::env::var("NORMA_E2E_AUDIO_FIXTURE").ok()
    );
    println!("permission_status={:?}", microphone_authorization_status());
    if mode == "status-only" {
        return;
    }
    let mut vad = SherpaVadClient::from_vad_model_dir(&vad_dir).expect("vad model");
    let (session, chunks) = match start_duplex_pcm_stream(None) {
        Ok(started) => started,
        Err(error) => {
            println!("start_error={error}");
            std::process::exit(2);
        }
    };
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let (mut total, mut voiced, mut speech_starts, mut spans) = (0usize, 0usize, 0usize, 0usize);
    while Instant::now() < deadline {
        let Ok(chunk) = chunks.recv_blocking() else {
            break;
        };
        total += 1;
        voiced += usize::from(chunk.bytes.iter().any(|byte| *byte != 0));
        if let Ok(observation) = vad.observe_pcm(&chunk) {
            speech_starts += observation
                .events
                .iter()
                .filter(|event| matches!(event, voice_input::VadEvent::SpeechStart))
                .count();
            spans += observation.completed_spans.len();
        }
    }
    drop(session);
    println!("chunks={total} non_silent_chunks={voiced} vad_speech_starts={speech_starts} vad_spans={spans}");
}
