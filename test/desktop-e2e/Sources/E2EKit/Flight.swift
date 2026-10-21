import CoreGraphics
import Foundation
import ImageIO

/// 拖拽派发的动画验收：不做文字识别，只比较像素变化，判断流星有没有从该出现的地方飞过。
extension Session {
    /// 只导出窗口画面，不跑文字识别（约几十毫秒），用于松手后的连续取样。
    public func snapshot() async throws -> CGImage {
        let png = FileManager.default.temporaryDirectory.appendingPathComponent("norma-e2e-flight-\(UUID().uuidString).png")
        defer { try? FileManager.default.removeItem(at: png) }
        _ = try await control(["op": "capture", "path": png.path])
        // 图像解码是惰性的：先把文件读进内存，临时文件随后被删除也不影响。
        guard let data = try? Data(contentsOf: png),
              let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            throw E2EError.env("导出窗口画面失败。")
        }
        return image
    }

    /// 把取样画面存进本次证据目录。
    public func saveSnapshot(_ image: CGImage, _ name: String) throws {
        guard let output else { throw E2EError.env("运行目录尚未建立") }
        let url = output.appendingPathComponent(name)
        guard let destination = CGImageDestinationCreateWithURL(url as CFURL, "public.png" as CFString, 1, nil) else {
            throw E2EError.env("无法编码截图 \(name)")
        }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else { throw E2EError.env("无法编码截图 \(name)") }
    }

    /// 与 `drag` 相同的轨迹，但移动到一半按 Esc 再松手：想法必须保持待办。
    public func dragCancellingWithEscape(local start: CGPoint, to end: CGPoint, on frame: Frame) async throws {
        try await activate()
        try await control(["op": "mouse", "phase": "move", "x": Double(start.x), "y": Double(start.y), "pressed": false])
        try await Task.sleep(nanoseconds: 100_000_000)
        try await control(["op": "mouse", "phase": "down", "x": Double(start.x), "y": Double(start.y), "pressed": false])
        for step in 1...8 {
            let progress = CGFloat(step) / 8
            let point = CGPoint(x: start.x + (end.x - start.x) * progress, y: start.y + (end.y - start.y) * progress)
            try await control(["op": "mouse", "phase": "move", "x": Double(point.x), "y": Double(point.y), "pressed": true])
            try await Task.sleep(nanoseconds: 25_000_000)
        }
        try await shortcut(key: 53, modifiers: [])
        try await Task.sleep(nanoseconds: 80_000_000)
        try await control(["op": "mouse", "phase": "up", "x": Double(end.x), "y": Double(end.y), "pressed": false])
    }

    /// 按下、移动并松手，松手后立即按给定间隔（毫秒）连续取样，返回每个时刻的画面。
    public func dragSampling(local start: CGPoint, to end: CGPoint, on frame: Frame, afterMs: [UInt64]) async throws -> [CGImage] {
        try await drag(local: start, to: end, on: frame)
        var images: [CGImage] = []
        var elapsed: UInt64 = 0
        for target in afterMs {
            if target > elapsed { try await Task.sleep(nanoseconds: (target - elapsed) * 1_000_000) }
            elapsed = max(elapsed, target)
            images.append(try await snapshot())
        }
        return images
    }
}

/// 两张同尺寸画面在窗口坐标矩形内“明显变化”的像素数（RGB 差之和超过阈值）。
public func changedPixels(_ a: CGImage, _ b: CGImage, in rect: CGRect, windowSize: CGSize, threshold: Int = 120) -> Int {
    guard a.width == b.width, a.height == b.height, windowSize.width > 0, windowSize.height > 0 else { return 0 }
    let scaleX = CGFloat(a.width) / windowSize.width
    let scaleY = CGFloat(a.height) / windowSize.height
    let area = CGRect(x: rect.minX * scaleX, y: rect.minY * scaleY, width: rect.width * scaleX, height: rect.height * scaleY)
        .intersection(CGRect(x: 0, y: 0, width: a.width, height: a.height))
    guard !area.isNull, area.width >= 1, area.height >= 1 else { return 0 }
    func bytes(_ image: CGImage) -> [UInt8]? {
        var data = [UInt8](repeating: 0, count: image.width * image.height * 4)
        guard let context = CGContext(
            data: &data, width: image.width, height: image.height, bitsPerComponent: 8, bytesPerRow: image.width * 4,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { return nil }
        context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        return data
    }
    guard let left = bytes(a), let right = bytes(b) else { return 0 }
    var count = 0
    for y in Int(area.minY)..<Int(area.maxY) {
        for x in Int(area.minX)..<Int(area.maxX) {
            let index = (y * a.width + x) * 4
            let delta = abs(Int(left[index]) - Int(right[index])) + abs(Int(left[index + 1]) - Int(right[index + 1])) + abs(Int(left[index + 2]) - Int(right[index + 2]))
            if delta > threshold { count += 1 }
        }
    }
    return count
}
