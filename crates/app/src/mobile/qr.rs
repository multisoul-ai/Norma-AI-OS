use anyhow::{ensure, Context as _, Result};
use image::{Rgba, RgbaImage};
use objc2_core_image::{kCIFormatRGBA8, CIContext, CIFilter};
use objc2_foundation::{ns_string, NSData, NSObjectNSKeyValueCoding};
use std::ptr::NonNull;

pub(super) fn pixels(payload: &str) -> Result<RgbaImage> {
    ensure!(
        !payload.is_empty() && payload.len() <= 4096,
        "二维码内容无效"
    );
    // SAFETY: fixed system filter and documented KVC keys/types; no untrusted selector names.
    let modules = unsafe {
        let filter = CIFilter::filterWithName(ns_string!("CIQRCodeGenerator"))
            .context("系统二维码不可用")?;
        filter.setValue_forKey(
            Some(&NSData::with_bytes(payload.as_bytes())),
            ns_string!("inputMessage"),
        );
        filter.setValue_forKey(Some(ns_string!("M")), ns_string!("inputCorrectionLevel"));
        let output = filter.outputImage().context("无法生成二维码")?;
        let bounds = output.extent();
        let side = bounds.size.width as u32;
        ensure!(
            (21..=177).contains(&side) && bounds.size.width == bounds.size.height,
            "二维码尺寸无效"
        );
        let mut pixels = RgbaImage::new(side, side);
        CIContext::context().render_toBitmap_rowBytes_bounds_format_colorSpace(
            &output,
            NonNull::new(pixels.as_mut_ptr().cast()).context("二维码像素为空")?,
            (side * 4) as isize,
            bounds,
            kCIFormatRGBA8,
            None,
        );
        pixels
    };
    // Four white modules on every edge guarantee a quiet zone; nearest scaling preserves cells.
    let mut bordered =
        RgbaImage::from_pixel(modules.width() + 8, modules.height() + 8, Rgba([255; 4]));
    image::imageops::replace(&mut bordered, &modules, 4, 4);
    Ok(image::imageops::resize(
        &bordered,
        bordered.width() * 4,
        bordered.height() * 4,
        image::imageops::FilterType::Nearest,
    ))
}

/// 原生 QR 像素可被独立 Vision 解码，边缘有完整 quiet zone，超大内容明确失败。
///
/// 数据构造：512 个 ASCII 字节模拟较长 QR；四模块边界 × 四倍像素 = 16px 白边。
/// 执行过程：Core Image 生成并保存临时 PNG → Swift/Vision 读取并识别 → 校验内容和白边 → 拒绝 4097 字节。
/// 预期结果：正文原样返回、无镜像或模糊损坏；边界纯白，超过 4 KiB 上限不能悄悄截断。
#[cfg(test)]
#[test]
fn native_qr_pixels_round_trip_through_vision() {
    let payload = "norma-qr-test-".to_string() + &"a".repeat(512);
    let image = pixels(&payload).expect("系统生成 QR");
    assert_eq!(
        *image.get_pixel(15, 15),
        Rgba([255; 4]),
        "16px quiet zone 必须保持纯白"
    );
    let temp = tempfile::tempdir().expect("隔离 QR 目录");
    let path = temp.path().join("code.png");
    image.save(&path).expect("保存测试图片");
    let output = std::process::Command::new("/usr/bin/swift")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/check-norma-qr.swift"
        ))
        .arg(path)
        .output()
        .expect("独立 Apple Vision 验证");
    assert!(
        output.status.success(),
        "原生相机识别协议必须可解码生成的像素"
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF8").trim(),
        payload,
        "内容不能被镜像、裁切或编码转换破坏"
    );
    assert!(
        pixels(&"x".repeat(4097)).is_err(),
        "超过 4096 bytes 必须拒绝，不能截断二维码凭据"
    );
}
