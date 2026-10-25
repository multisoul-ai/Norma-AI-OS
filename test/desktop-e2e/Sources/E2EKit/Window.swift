import CoreGraphics
import Foundation

/// 启动时控制 socket 可早于窗口开放；只读 window 探测不重放任何输入。
/// 134 次探测之间最多等待 133 × 150ms = 19.95s，其他拒绝及进程死亡立即传播。
func waitForControlWindow(
    attempts: Int = 134,
    intervalNanoseconds: UInt64 = 150_000_000,
    operation: () async throws -> [String: Any]?
) async throws -> [String: Any] {
    for attempt in 0..<attempts {
        if let response = try await operation() {
            if response["ok"] as? Bool == true { return response }
            let reason = response["error"] as? String ?? "unknown"
            guard reason == "no window" else {
                throw E2EError.product("App 控制通道拒绝 window：\(reason)")
            }
        }
        if attempt + 1 < attempts {
            try await Task.sleep(nanoseconds: intervalNanoseconds)
        }
    }
    throw E2EError.driver("启动等待预算耗尽，窗口未就绪（控制通道未连接或持续返回 no window）")
}

extension Session {
    /// 通过 App 控制通道调整本次隔离窗口的尺寸。
    public func resize(width: CGFloat, height: CGFloat) async throws {
        guard let channel else { throw E2EError.env("控制通道尚未建立") }
        let response = try await waitForControlWindow {
            try self.ensureRunning()
            return try channel.send(["op": "window"])
        }
        _ = try controlWindow(response)
        recordDriverEvent("window", "ready", "窗口就绪后才发送 resize")
        try await control(["op": "resize", "width": Double(width), "height": Double(height)])
        let deadline = Date().addingTimeInterval(5)
        var actual = try await currentWindow().frame
        while Date() < deadline {
            actual = try await currentWindow().frame
            if abs(actual.width - width) <= 2 && abs(actual.height - height) <= 2 { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        try save(try await capturePixels(), "resize-failure.png")
        throw E2EError.product("隔离窗口未达到期望尺寸 \(width)×\(height)，实际 \(actual.width)×\(actual.height)")
    }
}
