//! Video feasibility spike: AVAssetReader → 420f CVPixelBuffer loop.
//!
//! This is a throwaway spike (wallpaper Task 1). The `VideoDecoder` interface
//! frozen here is the contract adopted verbatim by wallpaper Tasks 10–13; any
//! signature change requires explicit escalation.
//!
//! GATED to macOS: AVFoundation / CoreMedia / CoreVideo are only available there.
//!
//! Approach (high-level objc2 crate, NOT raw `msg_send!`):
//!   - `AVURLAsset` opens the file.
//!   - `AVAssetReader` + `AVAssetReaderTrackOutput` decode the first video
//!     track straight to the requested CVPixelBuffer format via
//!     `outputSettings = { PixelFormatType → 420f }` (for the decode loop) or
//!     `{ PixelFormatType → 32BGRA }` (for `first_frame_bgra` thumbnails).
//!   - `copyNextSampleBuffer` → raw `CMSampleBufferGetImageBuffer` (extern C,
//!     linked via objc2-core-media's `#[link(name="CoreMedia")]`) → the buffer
//!     is wrapped into a `core_video::pixel_buffer::CVPixelBuffer` (the exact
//!     type GPUI 0.2.2's `surface()` consumes) under the CoreFoundation *get*
//!     rule (retained), so the frame outlives the sample buffer.
//!   - On EOF the reader's `status` becomes `Completed`; we rebuild a fresh
//!     reader + output against the same asset/track to loop.
//!
//! GATED to macOS via the parent `#[cfg(target_os = "macos")] mod video_decoder;`
//! in `mod.rs`; no inner `#![cfg]` is needed (it would duplicate that gate).

use anyhow::{anyhow, bail, ensure, Result};
use core_foundation::base::TCFType;
use core_video::pixel_buffer::{
    kCVPixelBufferLock_ReadOnly, kCVPixelFormatType_32BGRA,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, CVPixelBuffer, CVPixelBufferRef,
};
use core_video::r#return::kCVReturnSuccess;
use image::RgbaImage;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVAssetTrack, AVMediaTypeVideo,
    AVURLAsset,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use std::ffi::c_void;
use std::path::Path;

/// NSString value of CoreVideo's `kCVPixelBufferPixelFormatTypeKey` output
/// settings key. NSDictionary compares by string value, so a literal NSString
/// is equivalent to the toll-free-bridged CFString constant.
const PIXEL_FORMAT_KEY: &str = "PixelFormatType";

extern "C-unwind" {
    /// `CMSampleBufferGetImageBuffer` — CoreMedia C function returning a
    /// get-rule `CVImageBufferRef` (== `CVPixelBufferRef` for decoded video),
    /// or NULL when the sample buffer carries no image. Resolved from
    /// CoreMedia.framework, linked by `objc2-core-media`.
    fn CMSampleBufferGetImageBuffer(sbuf: *mut c_void) -> *mut c_void;
}

/// A decoded video frame. `buffer` is guaranteed to be 420f, the only pixel
/// format GPUI 0.2.2's Metal surface renderer accepts.
pub(crate) struct DecodedVideoFrame {
    pub buffer: CVPixelBuffer,
}

/// Loops an mp4/mov via AVAssetReader, emitting 420f CVPixelBuffers ready for
/// `gpui::surface()`. Uses the first video track and rebuilds the reader at
/// EOF to loop indefinitely.
pub(crate) struct VideoDecoder {
    asset: Retained<AVURLAsset>,
    reader: Retained<AVAssetReader>,
    output: Retained<AVAssetReaderTrackOutput>,
}

impl VideoDecoder {
    /// Open `path` (mp4/mov). Returns Err if unreadable / unsupported.
    pub(crate) fn open(path: &Path) -> Result<Self> {
        // SAFETY: open_url_asset only constructs an AVURLAsset; no aliasing.
        let asset = unsafe { open_url_asset(path) }?;
        let track = first_video_track(&asset)?;
        let reader = new_reader(&asset)?;
        let output = new_track_output(&track, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange)?;
        attach_and_start(&reader, &output)?;
        Ok(Self {
            asset,
            reader,
            output,
        })
    }

    /// Next frame in a 420f CVPixelBuffer; loops at EOF.
    ///
    /// `Duration`-hint semantics from the frozen interface are intentionally
    /// not throttled here: the decoder advances to the next available frame as
    /// fast as AVFoundation decodes. Callers (Task 11) gate cadence externally.
    pub(crate) fn next_frame(&mut self) -> Result<DecodedVideoFrame> {
        loop {
            let next = unsafe { self.output.copyNextSampleBuffer() };
            match next {
                Some(sb) => {
                    let sbuf_ptr = Retained::as_ptr(&sb) as *mut c_void;
                    let img_ptr = unsafe { CMSampleBufferGetImageBuffer(sbuf_ptr) };
                    if img_ptr.is_null() {
                        // Marker-only sample buffer (no image); advance.
                        continue;
                    }
                    // SAFETY: img_ptr is a get-rule CVImageBufferRef returned
                    // by CoreMedia; for video track output it is a
                    // CVPixelBufferRef. wrap_under_get_rule retains it so the
                    // frame is valid after the sample buffer drops.
                    let buffer = unsafe {
                        <CVPixelBuffer as TCFType>::wrap_under_get_rule(img_ptr as CVPixelBufferRef)
                    };
                    return Ok(DecodedVideoFrame { buffer });
                }
                None => {
                    let status = unsafe { self.reader.status() };
                    if status == AVAssetReaderStatus::Completed {
                        self.restart_reader()?;
                        continue;
                    }
                    let err = unsafe { self.reader.error() };
                    bail!("AVAssetReader stopped (status={:?}): {:?}", status, err);
                }
            }
        }
    }

    /// Single BGRA first-frame for thumbnails.
    ///
    /// SPIKE DEVIATION (documented): the brief suggested `AVAssetImageGenerator`.
    /// We instead reuse `AVAssetReader` with `32BGRA` output, which avoids the
    /// CGImage → bitmap-context conversion and is self-contained. The frozen
    /// *signature* (returns `image::RgbaImage`) is unchanged.
    pub(crate) fn first_frame_bgra(path: &Path) -> Result<RgbaImage> {
        // SAFETY: see open().
        let asset = unsafe { open_url_asset(path) }?;
        let track = first_video_track(&asset)?;
        let reader = new_reader(&asset)?;
        let output = new_track_output(&track, kCVPixelFormatType_32BGRA)?;
        attach_and_start(&reader, &output)?;

        loop {
            let next = unsafe { output.copyNextSampleBuffer() };
            let Some(sb) = next else {
                bail!("video has no decodable frame");
            };
            let sbuf_ptr = Retained::as_ptr(&sb) as *mut c_void;
            let img_ptr = unsafe { CMSampleBufferGetImageBuffer(sbuf_ptr) };
            if img_ptr.is_null() {
                continue;
            }
            let buffer = unsafe {
                <CVPixelBuffer as TCFType>::wrap_under_get_rule(img_ptr as CVPixelBufferRef)
            };
            return bgra_pixel_buffer_to_rgba(&buffer);
        }
    }

    /// Rebuild reader + output to loop. AVAssetReader is single-use.
    fn restart_reader(&mut self) -> Result<()> {
        // SAFETY: cancelReading is idempotent on a completed reader.
        unsafe { self.reader.cancelReading() };
        let track = unsafe { self.output.track() };
        let reader = new_reader(&self.asset)?;
        let output = new_track_output(&track, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange)?;
        attach_and_start(&reader, &output)?;
        self.reader = reader;
        self.output = output;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Open an AVURLAsset for the given file path.
///
/// # SAFETY
/// objc2 marks `URLAssetWithURL:options:` as `unsafe` because the `options`
/// dictionary's generics are caller-checked. We pass `None` for options,
/// which is always valid.
unsafe fn open_url_asset(path: &Path) -> Result<Retained<AVURLAsset>> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("video path is not valid UTF-8"))?;
    let ns_path = NSString::from_str(path_str);
    let url = NSURL::fileURLWithPath(&ns_path);
    Ok(AVURLAsset::URLAssetWithURL_options(&url, None))
}

/// First video track of `asset`. The deprecated sync `tracksWithMediaType:`
/// is avoided (clippy `-D deprecated` + the repo's no-`#[allow]` rule); instead
/// we pull ALL tracks via the non-deprecated `tracks` accessor and filter
/// client-side by `AVAssetTrack::mediaType`. For local file-backed assets this
/// is a synchronous, in-memory lookup, so blocking is safe here.
fn first_video_track(asset: &AVURLAsset) -> Result<Retained<AVAssetTrack>> {
    // SAFETY: AVMediaTypeVideo is a process-lifetime extern NSString constant
    // populated by AVFoundation at framework load; reading it after the crate
    // has linked AVFoundation (which it has) is sound.
    let video_media_type = unsafe { AVMediaTypeVideo }
        .ok_or_else(|| anyhow!("AVMediaTypeVideo static unavailable"))?;
    // SAFETY: `tracks` on a file-backed AVURLAsset is a synchronous, safe call
    // (the deprecated `tracksWithMediaType:` is replaced by `tracks` +
    // client-side media-type filtering).
    let all_tracks = unsafe { asset.tracks() };
    for track in all_tracks.iter() {
        // SAFETY: mediaType is a non-deprecated, synchronous accessor.
        let mt = unsafe { track.mediaType() };
        // Compare NSString *values* (not pointers) via isEqualToString:.
        if mt.isEqualToString(video_media_type) {
            // FirstObject-equivalent: return the first video track encountered.
            return Ok(track);
        }
    }
    Err(anyhow!("asset has no video track"))
}

fn new_reader(asset: &AVURLAsset) -> Result<Retained<AVAssetReader>> {
    // SAFETY: asset is a concrete AVURLAsset; the receiver param is satisfied.
    unsafe { AVAssetReader::assetReaderWithAsset_error(asset) }
        .map_err(|e| anyhow!("AVAssetReader init failed: {:?}", e))
}

/// Build an `AVAssetReaderTrackOutput` decoding to `pixel_format`.
fn new_track_output(
    track: &AVAssetTrack,
    pixel_format: u32,
) -> Result<Retained<AVAssetReaderTrackOutput>> {
    let key = NSString::from_str(PIXEL_FORMAT_KEY);
    let value_any: Retained<AnyObject> = NSNumber::new_u32(pixel_format).into();
    let settings =
        NSDictionary::<NSString, AnyObject>::from_retained_objects(&[&*key], &[value_any]);
    // SAFETY: output_settings is a properly-typed NSDictionary<NSString, AnyObject>;
    // `track` is a video track owned by the asset that the reader will bind to.
    let output = unsafe {
        AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(
            track,
            Some(&settings),
        )
    };
    Ok(output)
}

/// Add `output` to `reader` and start reading; Err on rejection / start failure.
fn attach_and_start(
    reader: &Retained<AVAssetReader>,
    output: &Retained<AVAssetReaderTrackOutput>,
) -> Result<()> {
    // SAFETY: both objects are freshly created and not yet attached.
    unsafe {
        if !reader.canAddOutput(output) {
            bail!("AVAssetReader rejected the video track output");
        }
        reader.addOutput(output);
        if !reader.startReading() {
            let err = reader.error();
            bail!("AVAssetReader.startReading failed: {:?}", err);
        }
    }
    Ok(())
}

/// Copy a 32BGRA CVPixelBuffer into an `image::RgbaImage` (swapping B↔R).
fn bgra_pixel_buffer_to_rgba(buf: &CVPixelBuffer) -> Result<RgbaImage> {
    let lock_status = buf.lock_base_address(kCVPixelBufferLock_ReadOnly);
    ensure!(
        lock_status == kCVReturnSuccess,
        "CVPixelBufferLockBaseAddress failed: {}",
        lock_status
    );
    let outcome = (|| -> Result<RgbaImage> {
        let width = buf.get_width();
        let height = buf.get_height();
        let bytes_per_row = buf.get_bytes_per_row();
        // SAFETY: lock succeeded; base address is valid until unlock.
        let base = unsafe { buf.get_base_address() };
        ensure!(!base.is_null(), "CVPixelBuffer base address is null");
        let mut img = RgbaImage::new(width as u32, height as u32);
        for y in 0..height {
            // SAFETY: row y spans [y*bpr, y*bpr + width*4) within the locked buffer.
            let row = unsafe { (base as *const u8).add(y * bytes_per_row) };
            for x in 0..width {
                // SAFETY: pixel x is within the row (width*4 ≤ bytes_per_row).
                let (b, g, r, a) = unsafe {
                    (
                        *row.add(x * 4),
                        *row.add(x * 4 + 1),
                        *row.add(x * 4 + 2),
                        *row.add(x * 4 + 3),
                    )
                };
                img.put_pixel(x as u32, y as u32, image::Rgba([r, g, b, a]));
            }
        }
        Ok(img)
    })();
    let _ = buf.unlock_base_address(kCVPixelBufferLock_ReadOnly);
    outcome
}
