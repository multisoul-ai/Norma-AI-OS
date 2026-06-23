use std::time::Duration;
use voice_macos::AppleSpeechProvider;

fn main() {
    let locale = AppleSpeechProvider::default_locale();
    eprintln!("starting Apple Speech smoke test with locale={locale}");
    match AppleSpeechProvider::start_recording(locale) {
        Ok(mut session) => {
            std::thread::sleep(Duration::from_millis(300));
            session.cancel();
            eprintln!("Apple Speech smoke test cancelled cleanly");
        }
        Err(error) => {
            eprintln!("Apple Speech smoke test failed without crashing: {error}");
        }
    }
}
