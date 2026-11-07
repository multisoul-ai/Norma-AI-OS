import CoreGraphics
import Foundation

public struct NativeWindowInfo {
    public let number: Int
    public let width: Double
    public let height: Double
}

extension Session {
    /// Query the macOS compositor, not a product preference or GPUI visibility flag.
    public func nativeWindowIsOnScreen(_ number: Int) throws -> Bool {
        guard let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else {
            throw E2EError.env("无法读取系统可见窗口列表")
        }
        return windows.contains { ($0[kCGWindowNumber as String] as? NSNumber)?.intValue == number }
    }

    /// 原生窗口元数据，用于选择独立的面板；不读取产品内部状态。
    public func nativeWindows() async throws -> [NativeWindowInfo] {
        let response = try await control(["op": "windows"])
        guard let items = response["windows"] as? [[String: Any]] else {
            throw E2EError.driver("控制通道未返回原生窗口列表")
        }
        return try items.map { item in
            guard let number = item["number"] as? NSNumber,
                  let width = item["width"] as? NSNumber,
                  let height = item["height"] as? NSNumber else {
                throw E2EError.driver("原生窗口元数据不完整")
            }
            return NativeWindowInfo(number: number.intValue, width: width.doubleValue, height: height.doubleValue)
        }
    }

    public func selectNativeWindow(_ number: Int?) async throws {
        let value: Any = number.map { NSNumber(value: $0) } ?? NSNull()
        try await control(["op": "select_window", "number": value])
    }
}
