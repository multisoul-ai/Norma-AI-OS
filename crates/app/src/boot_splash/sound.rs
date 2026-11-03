//! Boot soundtrack: one AVAudioPlayer on its own thread (file load and playback stay off the
//! GPUI foreground). It is prepared at splash start and cued when the first video frame lands.

use objc2::rc::autoreleasepool;
use objc2::AnyThread;
use objc2_avf_audio::AVAudioPlayer;
use objc2_foundation::{NSString, NSURL};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// Matches the video's first-frame timeout plus slack; an uncued player is dropped.
const CUE_TIMEOUT: Duration = Duration::from_secs(2);
/// Let the baked-in tail fade finish after the video length.
const TAIL: Duration = Duration::from_millis(500);

enum Cue {
    Play,
    FadeOut(Duration),
}

pub(super) struct BootSound {
    cues: mpsc::Sender<Cue>,
    playing: bool,
}

impl BootSound {
    pub(super) fn load(path: PathBuf, length: Duration) -> Self {
        let (cues, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("boot_sound".to_string())
            .spawn(move || autoreleasepool(|_| run(&path, length, rx)));
        if let Err(error) = spawned {
            tracing::warn!(%error, "boot soundtrack thread failed to start");
        }
        Self {
            cues,
            playing: false,
        }
    }

    /// Idempotent: the splash calls this on every tick that has a frame.
    pub(super) fn play(&mut self) {
        if !self.playing {
            self.playing = true;
            let _ = self.cues.send(Cue::Play);
        }
    }

    pub(super) fn fade_out(&self, length: Duration) {
        let _ = self.cues.send(Cue::FadeOut(length));
    }
}

fn run(path: &Path, length: Duration, cues: mpsc::Receiver<Cue>) {
    let Some(path_str) = path.to_str() else {
        return;
    };
    let url = NSURL::fileURLWithPath(&NSString::from_str(path_str));
    // SAFETY: a file URL is a valid argument; errors come back as Err.
    let player = match unsafe {
        AVAudioPlayer::initWithContentsOfURL_error(AVAudioPlayer::alloc(), &url)
    } {
        Ok(player) => player,
        Err(error) => {
            tracing::warn!(?path, error = %error.localizedDescription(), "boot soundtrack failed to load");
            return;
        }
    };
    // SAFETY: plain AVAudioPlayer calls on the thread that owns the player.
    unsafe { player.prepareToPlay() };
    if !matches!(cues.recv_timeout(CUE_TIMEOUT), Ok(Cue::Play)) {
        return;
    }
    // SAFETY: same owning thread as above.
    unsafe { player.play() };
    // A dropped splash (window closed) disconnects the channel and stops at once.
    if let Ok(Cue::FadeOut(fade)) = cues.recv_timeout(length + TAIL) {
        // SAFETY: same owning thread as above.
        unsafe { player.setVolume_fadeDuration(0.0, fade.as_secs_f64()) };
        std::thread::sleep(fade);
    }
    // SAFETY: same owning thread as above.
    unsafe { player.stop() };
}
