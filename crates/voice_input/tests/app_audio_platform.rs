// Exercise the actual App audio adapters without unrelated storage/agent/GPUI tests.
#[cfg(windows)]
#[path = "../../app/src/app/voice/playback.rs"]
mod playback;

mod recognition {
    include!("../../app/src/app/voice/recognition.rs");
    // Keep the process-global fixture isolated from unrelated App test consumers.
    mod capture_tests {
        include!("../../app/tests/unit/recognition_capture_tests.rs");
    }
}
