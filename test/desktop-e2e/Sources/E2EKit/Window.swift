import ApplicationServices
import CoreGraphics
import Foundation

extension Session {
    /// 调整本次隔离进程唯一窗口的尺寸，不读取产品控件树或注入 App 测试钩子。
    public func resize(width: CGFloat, height: CGFloat) async throws {
        try await activate()
        guard let pid = app?.processIdentifier else { throw E2EError.product("隔离进程未启动") }
        let application = AXUIElementCreateApplication(pid)
        var raw: CFTypeRef?
        guard AXUIElementCopyAttributeValue(application, kAXWindowsAttribute as CFString, &raw) == .success,
              let windows = raw as? [AXUIElement], windows.count == 1,
              let window = windows.first else {
            throw E2EError.product("调整尺寸要求隔离进程恰有一个窗口，已避免操作其他窗口")
        }
        var size = CGSize(width: width, height: height)
        guard let value = AXValueCreate(.cgSize, &size),
              AXUIElementSetAttributeValue(window, kAXSizeAttribute as CFString, value) == .success else {
            throw E2EError.product("系统未接受隔离窗口尺寸 \(width)×\(height)")
        }
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
