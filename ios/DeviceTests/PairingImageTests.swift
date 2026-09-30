import XCTest
import CoreImage.CIFilterBuiltins
import UIKit
@testable import Norma

final class PairingImageTests: XCTestCase {
    /// 图片中的单个二维码保留原文，多码、损坏和超限图片必须拒绝。
    ///
    /// 数据构造：二维码原文为固定测试字符串；每个码为300×300像素，双码画布660×340；
    ///   20MiB+1 = 20×1024×1024+1字节，超过图片导入上限。
    /// 执行过程：生成单码PNG → 解码核对原文 → 解码双码/空白/损坏/超限数据。
    /// 预期结果：单码内容不改写；不能从多台电脑中擅自选一台，也不能将无效图片当成配对成功。
    @MainActor func testPairingImageDecodingAndRejection() throws {
        let first = "norma-pairing-image-first"
        let second = "norma-pairing-image-second"
        XCTAssertEqual(try PairingImageCode.decode(png([first])), first, "图片识别必须返回完整原文，由现有配对逻辑验证")
        XCTAssertThrowsError(try PairingImageCode.decode(png([first, second]))) {
            XCTAssertEqual($0 as? PairingImageError, .multipleCodes, "多码图片不能擅自选择授权目标")
        }
        XCTAssertThrowsError(try PairingImageCode.decode(png([]))) {
            XCTAssertEqual($0 as? PairingImageError, .noCode, "无二维码时应提示重新选择")
        }
        XCTAssertThrowsError(try PairingImageCode.decode(Data("broken image".utf8)), "损坏图片不能产生配对原文")
        XCTAssertThrowsError(try PairingImageCode.decode(Data(repeating: 0, count: 20 * 1024 * 1024 + 1))) {
            XCTAssertEqual($0 as? PairingImageError, .tooLarge, "编码数据超限必须在图片展开前拒绝")
        }
    }

    @MainActor private func png(_ texts: [String]) throws -> Data {
        let images = try texts.map { text in
            let filter = CIFilter.qrCodeGenerator()
            filter.message = Data(text.utf8)
            let output = try XCTUnwrap(filter.outputImage, "系统应能生成测试二维码")
            let pixels = try XCTUnwrap(CIContext().createCGImage(output, from: output.extent), "测试二维码应可渲染")
            return UIImage(cgImage: pixels)
        }
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        return UIGraphicsImageRenderer(size: CGSize(width: 660, height: 340), format: format).pngData { context in
            UIColor.white.setFill(); context.fill(CGRect(x: 0, y: 0, width: 660, height: 340))
            context.cgContext.interpolationQuality = .none
            for (index, image) in images.enumerated() {
                image.draw(in: CGRect(x: 20 + index * 320, y: 20, width: 300, height: 300))
            }
        }
    }
}
