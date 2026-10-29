import Foundation

extension Session {
    /// 只发送原始鼠标事件：按住截图后移到空白处松开，避免触发被测按钮。
    public func capturePressed(local point: CGPoint, on frame: Frame) async throws -> Frame {
        try await activate()
        let window = try await currentWindow()
        guard window.id == frame.windowID else { throw E2EError.product("按下前窗口已切换") }
        try await control(["op": "mouse", "phase": "move", "x": Double(point.x), "y": Double(point.y), "pressed": false])
        try await control(["op": "mouse", "phase": "down", "x": Double(point.x), "y": Double(point.y), "pressed": false])
        do {
            // ACK只确认输入已处理；等待按下状态与WindowServer合成帧，不能截成前一张hover画面。
            try await Task.sleep(nanoseconds: 150_000_000)
            let pressed = try await capture()
            try await control(["op": "mouse", "phase": "move", "x": 12.0, "y": 100.0, "pressed": true])
            try await control(["op": "mouse", "phase": "up", "x": 12.0, "y": 100.0, "pressed": false])
            return pressed
        } catch {
            _ = try? await control(["op": "mouse", "phase": "up", "x": 12.0, "y": 100.0, "pressed": false])
            throw error
        }
    }
}
