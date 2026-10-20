import CoreGraphics
import Foundation

extension Session {
    /// 通过 App 控制通道调整本次隔离窗口的尺寸。
    public func resize(width: CGFloat, height: CGFloat) async throws {
        try await control(["op": "resize", "width": Double(width), "height": Double(height)])
        let deadline = Date().addingTimeInterval(5)
        var actual = try await currentWindow().frame
        while Date() < deadline {
            actual = try await currentWindow().frame
            if abs(actual.width - width) <= 2 && abs(actual.height - height) <= 2 { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        try save(try await capture(), "resize-failure.png")
        throw E2EError.product("隔离窗口未达到期望尺寸 \(width)×\(height)，实际 \(actual.width)×\(actual.height)")
    }
}
