import CoreGraphics
import Testing
@testable import E2EKit

/// 场景：动画冻结在最后一帧，连续两张截图完全相同仍不能通过消失验收。
/// 数据构造：100×100 黑底中央保留 20×20 白块（400 像素），允许噪声为 150。
/// 执行过程：分别把残留帧和干净帧传入同一个画面收尾验收入口。
/// 预期结果：残留帧报 PRODUCT，恢复基线通过；不能只因帧间差为 0 就放行。
@Test func frozenEffectCannotPassClearance() throws {
    let size = CGSize(width: 100, height: 100)
    let area = CGRect(origin: .zero, size: size)
    let clean = try image(size: size, block: nil)
    let frozen = try image(size: size, block: CGRect(x: 40, y: 40, width: 20, height: 20))
    #expect(changedPixels(frozen, frozen, in: area, windowSize: size) == 0)
    #expect(changedPixels(clean, frozen, in: area, windowSize: size) == 400)
    do {
        try requireAnimationCleared(reference: clean, final: frozen, in: area, windowSize: size, tolerance: 150)
        Issue.record("冻结残留必须报错，不能通过消失验收")
    } catch let error as E2EError {
        #expect(error.kind == .product)
    }
    try requireAnimationCleared(reference: clean, final: clean, in: area, windowSize: size, tolerance: 150)
}

private func image(size: CGSize, block: CGRect?) throws -> CGImage {
    let context = try #require(CGContext(
        data: nil, width: Int(size.width), height: Int(size.height), bitsPerComponent: 8,
        bytesPerRow: Int(size.width) * 4, space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ))
    context.setFillColor(CGColor(gray: 0, alpha: 1))
    context.fill(CGRect(origin: .zero, size: size))
    if let block {
        context.setFillColor(CGColor(gray: 1, alpha: 1))
        context.fill(block)
    }
    return try #require(context.makeImage())
}
