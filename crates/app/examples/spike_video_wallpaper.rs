//! Spike binary: prove the `VideoDecoder` emits a loop of 420f CVPixelBuffers
//! that GPUI 0.2.2's `surface()` element can consume without panicking.
//!
//! Run (macOS, native arch):
//!   cargo run -p app --example spike_video_wallpaper
//!   MULTISOUL_SPIKE_VIDEO=/other.mp4 cargo run -p app --example spike_video_wallpaper
//!
//! Expected: prints `OK <w>x<h> 420f` 30 times, then exits 0.
//!
//! This example compiles the SAME frozen `video_decoder.rs` the app crate ships
//! (via `#[path]`), so it exercises the exact interface Tasks 10–13 adopt.

#[cfg(target_os = "macos")]
#[path = "../src/app/video_decoder.rs"]
mod video_decoder;

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use core_video::pixel_buffer::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange;
    use std::path::PathBuf;

    use video_decoder::VideoDecoder;

    let path = std::env::var("MULTISOUL_SPIKE_VIDEO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/Users/alan/Documents/壁纸/mut/1403512.mp4"));
    if !path.exists() {
        anyhow::bail!(
            "spike video not found: {} (set MULTISOUL_SPIKE_VIDEO)",
            path.display()
        );
    }

    let mut dec = VideoDecoder::open(&path)?;
    for i in 0..30 {
        let frame = dec.next_frame()?;
        let pf = frame.buffer.get_pixel_format();
        assert_eq!(
            pf, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
            "frame {}: not 420f (got 0x{:08x}); GPUI surface() would panic",
            i, pf
        );
        println!(
            "OK {}x{} 420f",
            frame.buffer.get_width(),
            frame.buffer.get_height()
        );
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("spike_video_wallpaper is macOS-only; nothing to do on this target.");
}
