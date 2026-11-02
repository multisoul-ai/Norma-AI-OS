#![cfg(windows)]
// Exercise the actual App audio adapters without unrelated storage/agent/GPUI tests.
#[path = "../../app/src/app/voice/playback.rs"]
mod playback;

#[path = "../../app/src/app/voice/recognition.rs"]
mod recognition;
