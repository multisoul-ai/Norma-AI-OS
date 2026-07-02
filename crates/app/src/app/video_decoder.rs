//! Video decoder glue: AVAssetReader → 420f surface frames + BGRA thumbnails.
//!
//! GATED to macOS: AVFoundation / CoreMedia / CoreVideo are only available there.
//!
//! Approach (high-level objc2 crate, NOT raw `msg_send!`):
//!   - `AVURLAsset` opens the file.
//!   - `AVAssetReader` + `AVAssetReaderTrackOutput` decode the first video
//!     track straight to `{ PixelFormatType → 420f }` for live playback.
//!   - `copyNextSampleBuffer` → raw `CMSampleBufferGetImageBuffer` (extern C,
//!     linked via objc2-core-media's `#[link(name="CoreMedia")]`) → retained
//!     `CVPixelBuffer` for GPUI `surface()` live rendering.
//!   - Thumbnail decoding separately asks AVFoundation for `32BGRA`, then copies
//!     that single frame into `image::RgbaImage`.
//!   - On EOF the reader's `status` becomes `Completed`; we rebuild a fresh
//!     reader + output against the same asset/track to loop.
//!
//! GATED to macOS via the parent `#[cfg(target_os = "macos")] mod video_decoder;`
//! in `mod.rs`; no inner `#![cfg]` is needed (it would duplicate that gate).

use anyhow::{anyhow, bail, ensure, Result};
use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};
use core_video::pixel_buffer::{
    kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferLock_ReadOnly,
    kCVPixelBufferMetalCompatibilityKey, kCVPixelFormatType_32BGRA,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, CVPixelBuffer, CVPixelBufferRef,
};
use core_video::pixel_buffer_io_surface::CVPixelBufferGetIOSurface;
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

/// A decoded live video frame. `buffer` is guaranteed to be 420f, the pixel
/// format GPUI 0.2.2's surface renderer accepts.
pub(crate) struct DecodedVideoFrame {
    buffer: CVPixelBuffer,
}

impl DecodedVideoFrame {
    pub(crate) fn from_buffer(buffer: CVPixelBuffer) -> Result<Self> {
        let actual = buffer.get_pixel_format();
        ensure!(
            actual == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
            "decoded live video frame pixel format must be 420f for gpui::surface, got {}",
            actual
        );
        ensure!(
            buffer.get_plane_count() == 2,
            "decoded live video frame must be bi-planar 420f for gpui::surface, got {} planes",
            buffer.get_plane_count()
        );
        ensure!(
            Self::buffer_has_io_surface(&buffer),
            "decoded live video frame must be IOSurface-backed for low-copy gpui::surface rendering"
        );
        Ok(Self { buffer })
    }

    pub(crate) fn buffer(&self) -> &CVPixelBuffer {
        &self.buffer
    }

    pub(crate) fn buffer_has_io_surface(buffer: &CVPixelBuffer) -> bool {
        // `CVPixelBufferGetIOSurface` returns a borrowed IOSurfaceRef. Do not
        // wrap it in `IOSurface`; core-video 0.4.3's convenience wrapper treats
        // the pointer as owned and can release CoreVideo-owned storage.
        unsafe { !CVPixelBufferGetIOSurface(buffer.as_concrete_TypeRef()).is_null() }
    }
}

// SAFETY: `DecodedVideoFrame` can only be constructed after validating the
// retained CoreVideo pixel buffer is 420f, which is the read-only IOSurface path
// consumed by GPUI 0.2.2 `surface()`. The wallpaper pipeline transfers ownership
// between the decoder worker and GPUI thread, never locks the base address and
// never mutates the buffer after construction.
unsafe impl Send for DecodedVideoFrame {}

/// Loops an mp4/mov via AVAssetReader, emitting 420f CVPixelBuffers ready for
/// `gpui::surface()`. Uses the first video track and rebuilds the reader at EOF.
pub(crate) struct VideoDecoder {
    asset: Retained<AVURLAsset>,
    reader: Retained<AVAssetReader>,
    output: Retained<AVAssetReaderTrackOutput>,
}

impl VideoDecoder {
    /// Open `path` for 420f CVPixelBuffer frames suitable for `gpui::surface()`.
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

    /// Next 420f frame; loops at EOF. Callers own cadence.
    pub(crate) fn next_frame(&mut self) -> Result<DecodedVideoFrame> {
        loop {
            let next = unsafe { self.output.copyNextSampleBuffer() };
            match next {
                Some(sb) => {
                    let sbuf_ptr = Retained::as_ptr(&sb) as *mut c_void;
                    let img_ptr = unsafe { CMSampleBufferGetImageBuffer(sbuf_ptr) };
                    if img_ptr.is_null() {
                        continue;
                    }
                    let buffer = unsafe {
                        <CVPixelBuffer as TCFType>::wrap_under_get_rule(img_ptr as CVPixelBufferRef)
                    };
                    return DecodedVideoFrame::from_buffer(buffer);
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
    let settings = new_video_output_settings(pixel_format);
    // SAFETY: output_settings is a properly-typed NSDictionary<NSString, AnyObject>;
    // `track` is a video track owned by the asset that the reader will bind to.
    let output = unsafe {
        AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(
            track,
            Some(&settings),
        )
    };
    // SAFETY: output is freshly created and not attached to a reader yet.
    unsafe { output.setAlwaysCopiesSampleData(false) };
    Ok(output)
}

fn new_video_output_settings(pixel_format: u32) -> Retained<NSDictionary<NSString, AnyObject>> {
    let pixel_format_key = NSString::from_str(PIXEL_FORMAT_KEY);
    // SAFETY: CoreVideo exposes these process-lifetime CFString constants after
    // the framework is loaded. We convert their string values into NSString keys
    // for objc2 Foundation's typed NSDictionary.
    let metal_key = unsafe { ns_string_from_cf_key(kCVPixelBufferMetalCompatibilityKey) };
    let iosurface_key = unsafe { ns_string_from_cf_key(kCVPixelBufferIOSurfacePropertiesKey) };

    let pixel_format_value: Retained<AnyObject> = NSNumber::new_u32(pixel_format).into();
    let metal_value: Retained<AnyObject> = NSNumber::new_bool(true).into();
    let iosurface_properties =
        NSDictionary::<NSString, AnyObject>::from_retained_objects::<NSString>(&[], &[]);
    let iosurface_value: Retained<AnyObject> = iosurface_properties.into();

    NSDictionary::<NSString, AnyObject>::from_retained_objects(
        &[&*pixel_format_key, &*metal_key, &*iosurface_key],
        &[pixel_format_value, metal_value, iosurface_value],
    )
}

/// Convert a CoreVideo CFString key into an NSString with the same value.
///
/// # SAFETY
/// `key` must be a valid CoreFoundation string constant that lives for the
/// duration of the process.
unsafe fn ns_string_from_cf_key(key: CFStringRef) -> Retained<NSString> {
    let cf_key = unsafe { <CFString as TCFType>::wrap_under_get_rule(key) };
    NSString::from_str(&cf_key.to_string())
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
