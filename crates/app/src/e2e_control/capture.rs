//! Capture the App's own window. macOS lets a process read its own windows
//! without Screen Recording permission; other processes' windows still need it.
use std::ffi::c_void;
use std::path::Path;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

type CreateWindowImage = unsafe extern "C" fn(CGRect, u32, u32, u32) -> *mut c_void;

const INCLUDING_WINDOW: u32 = 1 << 3;
const BOUNDS_IGNORE_FRAMING: u32 = 1 << 0;
const ALPHA_PREMULTIPLIED_LAST: u32 = 1;
const BYTE_ORDER_32_BIG: u32 = 4 << 12;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGImageGetWidth(image: *mut c_void) -> usize;
    fn CGImageGetHeight(image: *mut c_void) -> usize;
    fn CGImageRelease(image: *mut c_void);
    fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    fn CGColorSpaceRelease(space: *mut c_void);
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: *mut c_void,
        bitmap_info: u32,
    ) -> *mut c_void;
    fn CGContextDrawImage(context: *mut c_void, rect: CGRect, image: *mut c_void);
    fn CGContextRelease(context: *mut c_void);
}

/// The symbol is marked obsoleted in recent SDK headers but is still exported at runtime.
fn create_window_image() -> Option<CreateWindowImage> {
    let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"CGWindowListCreateImage".as_ptr()) };
    (!symbol.is_null())
        .then(|| unsafe { std::mem::transmute::<*mut c_void, CreateWindowImage>(symbol) })
}

pub(super) fn capture_window_png(window_number: u32, path: &Path) -> Result<(), String> {
    let create = create_window_image().ok_or("CGWindowListCreateImage unavailable")?;
    let null_rect = CGRect {
        x: f64::INFINITY,
        y: f64::INFINITY,
        width: 0.0,
        height: 0.0,
    };
    let image = unsafe {
        create(
            null_rect,
            INCLUDING_WINDOW,
            window_number,
            BOUNDS_IGNORE_FRAMING,
        )
    };
    if image.is_null() {
        return Err("window image unavailable".into());
    }
    let pixels = unsafe { rgba_pixels(image) };
    unsafe { CGImageRelease(image) };
    let (width, height, pixels) = pixels?;
    image::RgbaImage::from_raw(width, height, pixels)
        .ok_or("pixel buffer size mismatch")?
        .save(path)
        .map_err(|error| format!("write png: {error}"))
}

unsafe fn rgba_pixels(image: *mut c_void) -> Result<(u32, u32, Vec<u8>), String> {
    let width = unsafe { CGImageGetWidth(image) };
    let height = unsafe { CGImageGetHeight(image) };
    if width == 0 || height == 0 {
        return Err("empty window image".into());
    }
    let mut buffer = vec![0u8; width * height * 4];
    let space = unsafe { CGColorSpaceCreateDeviceRGB() };
    let context = unsafe {
        CGBitmapContextCreate(
            buffer.as_mut_ptr().cast(),
            width,
            height,
            8,
            width * 4,
            space,
            ALPHA_PREMULTIPLIED_LAST | BYTE_ORDER_32_BIG,
        )
    };
    unsafe { CGColorSpaceRelease(space) };
    if context.is_null() {
        return Err("bitmap context unavailable".into());
    }
    let rect = CGRect {
        x: 0.0,
        y: 0.0,
        width: width as f64,
        height: height as f64,
    };
    unsafe {
        CGContextDrawImage(context, rect, image);
        CGContextRelease(context);
    }
    for pixel in buffer.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        if alpha > 0 && alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * 255) / alpha).min(255) as u8;
            }
        }
    }
    Ok((width as u32, height as u32, buffer))
}
