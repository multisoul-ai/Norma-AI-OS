import AppKit
import CoreGraphics
import Foundation

/// 点击、拖拽、滚动和按键都通过 App 控制通道派发到隔离窗口内，坐标为窗口内坐标。
/// 事件只进入本次启动的 Norma，不会落到用户的其他窗口，也不占用系统键鼠。
extension Session {
    /// 默认只在 App 内把隔离窗口标记为 active，不抢系统前台、键盘或鼠标；
    /// 设 `NORMA_E2E_FOREGROUND=1` 才会真正激活 Norma（旧行为）。
    public func activate() async throws {
        try ensureRunning()
        let env = ProcessInfo.processInfo.environment
        if env["NORMA_E2E_NO_ACTIVATE"] == "1" { return }
        if env["NORMA_E2E_FOREGROUND"] == "1",
           let pid = app?.processIdentifier, let running = NSRunningApplication(processIdentifier: pid) {
            _ = running.activate(from: NSRunningApplication.current, options: [.activateAllWindows])
        }
        try await control(["op": "activate"])
        // 控制通道在窗口绘制出 active 帧后才应答，输入不依赖固定等待。
    }

    /// 模拟用户离开：只在 App 内把隔离窗口标记为不活跃。之后任何输入都会重新激活它。
    public func deactivate() async throws {
        try ensureRunning()
        try await control(["op": "deactivate"])
    }

    public func click(matching candidates: [String], pick: TextPick) async throws {
        try await sendClick {
            let frame = try await capture()
            guard let text = frame.pick(candidates, pick) else {
                throw E2EError.product("未识别到可点击文字：\(candidates.joined(separator: " / "))。可见：\(frame.summary)")
            }
            return try await clickPoint(local: text.center, on: frame, requery: candidates, pick: pick)
        }
    }

    public func click(local: CGPoint, on frame: Frame) async throws {
        try await sendClick { try await clickPoint(local: local, on: frame, requery: nil, pick: nil) }
    }

    /// 把指针移到窗口内某点并停留，供悬浮提示验收；不按下按键。
    public func hover(local point: CGPoint, on frame: Frame, dwell: UInt64 = 1_200_000_000) async throws {
        try await activate()
        let window = try await currentWindow()
        guard window.id == frame.windowID else {
            throw E2EError.product("悬浮前窗口已切换，请重新截图定位。")
        }
        try await mouse("move", CGPoint(x: point.x - 6, y: point.y))
        try await Task.sleep(nanoseconds: 80_000_000)
        try await mouse("move", point)
        try await Task.sleep(nanoseconds: dwell)
    }

    /// 保存真正的按下帧；无论截图是否成功，都在原窗口释放鼠标。
    public func press(local point: CGPoint, on frame: Frame, whilePressed: () async throws -> Void) async throws {
        try await activate()
        let window = try await currentWindow()
        guard window.id == frame.windowID else { throw E2EError.product("按下前窗口已切换") }
        try await mouse("move", point)
        do {
            try await mouse("down", point)
            // Mouse ACK now waits for the pressed frame; hold until review completes.
            try await whilePressed()
        } catch {
            try? await mouse("up", point)
            throw error
        }
        try await mouse("up", point)
    }

    /// 在本次隔离窗口里连续拖动，供抽屉、卡片等真实指针交互验收。
    public func drag(local start: CGPoint, to end: CGPoint, on frame: Frame) async throws {
        try await activate()
        let window = try await currentWindow()
        guard window.id == frame.windowID else {
            throw E2EError.product("拖拽前窗口已切换，请重新截图定位。")
        }
        try await mouse("move", start)
        try await Task.sleep(nanoseconds: 100_000_000)
        try await mouse("down", start)
        for step in 1...12 {
            let progress = CGFloat(step) / 12
            let point = CGPoint(
                x: start.x + (end.x - start.x) * progress,
                y: start.y + (end.y - start.y) * progress
            )
            try await mouse("move", point, pressed: true)
            try await Task.sleep(nanoseconds: 25_000_000)
        }
        try await mouse("up", end)
    }

    /// Send a pixel wheel event to the captured isolated App window.
    public func scroll(local point: CGPoint, pixels: Int32, on frame: Frame) async throws {
        let window = try await currentWindow()
        guard window.id == frame.windowID else {
            throw E2EError.product("滚动前窗口已切换，请重新截图定位。")
        }
        try await wheel(at: point, pixels: pixels)
    }

    /// Scroll over the settings content region.
    public func scroll(pixels: Int32) async throws {
        let window = try await currentWindow()
        try await wheel(at: CGPoint(x: window.frame.width * 0.75, y: window.frame.height * 0.6), pixels: pixels)
    }

    public func scroll(xFraction: CGFloat, yFraction: CGFloat, deltaY: Int32) async throws {
        let window = try await currentWindow()
        let point = CGPoint(x: window.frame.width * xFraction, y: window.frame.height * yFraction)
        try await wheel(at: point, pixels: deltaY)
        try await Task.sleep(nanoseconds: 200_000_000)
    }

    public func type(_ value: String) async throws {
        try await activate()
        try await control(["op": appKitInput ? "ns_text" : "text", "text": value])
        try await Task.sleep(nanoseconds: 60_000_000)
    }

    public func selectAll() async throws {
        try await hotkey(key: 0, modifiers: [0x37])
    }

    public func deleteSelection() async throws {
        try await hotkey(key: 51, modifiers: [])
    }

    public func shortcut(key: CGKeyCode, modifiers: [CGKeyCode]) async throws {
        try await hotkey(key: key, modifiers: modifiers)
    }

    /// 精选目录卡片的「打开」在操作按钮右侧。优先点识别到的省略号，否则用两者中心距 68pt。
    public func openFeaturedActions() async throws {
        let frame = try await capture()
        if frame.contains("重命名工作空间") || frame.contains("Rename Workspace") { return }
        if frame.exact("重命名") != nil || frame.exact("Rename") != nil { return }
        guard let open = frame.exact("打开") ?? frame.exact("Open") else {
            throw E2EError.product("目录卡片缺少“打开”，无法定位操作按钮。可见：\(frame.summary)")
        }
        let marker = frame.texts.first { text in
            let token = text.value.trimmingCharacters(in: .whitespacesAndNewlines)
            let dx = open.center.x - text.center.x
            let near = abs(text.center.y - open.center.y) < 28 && dx > 20 && dx < 110
            return near && (token == "⋯" || token == "..." || token == "…" || token == "·")
        }
        let local = marker?.center ?? CGPoint(x: open.center.x - 68, y: open.center.y)
        try await click(local: local, on: frame)
    }

    private func sendClick(resolve: () async throws -> CGPoint) async throws {
        let clickID = UUID().uuidString
        try await deliverClick(resolve: resolve, send: { phase, point in
            try await mouse(phase, point)
        }, record: { stage, detail in
            recordDriverEvent("click", stage, "id=\(clickID) \(detail)")
        })
    }

    private func clickPoint(local: CGPoint, on frame: Frame, requery: [String]?, pick: TextPick?) async throws -> CGPoint {
        try await activate()
        var point = local
        let window = try await currentWindow()
        if window.id != frame.windowID || abs(window.frame.width - frame.windowFrame.width) > 2 {
            guard let requery, let pick else {
                throw E2EError.product("点击前窗口已切换或改变尺寸，请重新截图定位。")
            }
            let fresh = try await capture()
            guard let text = fresh.pick(requery, pick) else {
                throw E2EError.product("重取窗口后未重新识别：\(requery.joined(separator: " / "))")
            }
            point = text.center
        }
        return point
    }

    /// Spike: NORMA_E2E_INPUT=ns routes mouse/text through AppKit NSWindow.sendEvent.
    private var appKitInput: Bool { ProcessInfo.processInfo.environment["NORMA_E2E_INPUT"] == "ns" }

    private func mouse(_ phase: String, _ point: CGPoint, pressed: Bool = false) async throws {
        try await control([
            "op": appKitInput ? "ns_mouse" : "mouse", "phase": phase,
            "x": Double(point.x), "y": Double(point.y), "pressed": pressed,
        ])
    }

    private func wheel(at point: CGPoint, pixels: Int32) async throws {
        try await mouse("move", point)
        try await control(["op": "scroll", "x": Double(point.x), "y": Double(point.y), "dy": Double(pixels)])
    }

    /// 按住（或传空数组松开）修饰键，对应 GPUI 的 ModifiersChanged；用于验证“按住时才出现”的提示。
    public func holdModifiers(_ modifiers: [CGKeyCode]) async throws {
        try await activate()
        try await control(["op": "modifiers", "modifiers": try modifiers.map(modifierName)])
        try await Task.sleep(nanoseconds: 250_000_000)
    }

    private func hotkey(key: CGKeyCode, modifiers: [CGKeyCode]) async throws {
        try await activate()
        if appKitInput {
            try await control([
                "op": "ns_key", "key_code": Int(key), "chars": try keyCharacters(key),
                "modifiers": try modifiers.map(modifierName),
            ])
            try await Task.sleep(nanoseconds: 30_000_000)
            return
        }
        try await control(["op": "key", "keystroke": try keystroke(key: key, modifiers: modifiers)])
        try await Task.sleep(nanoseconds: 30_000_000)
    }
}

/// 把场景使用的 macOS 虚拟键码转成 GPUI 按键描述，例如 Cmd+D → "cmd-d"。
func keystroke(key: CGKeyCode, modifiers: [CGKeyCode]) throws -> String {
    let names: [CGKeyCode: String] = [
        21: "4", 23: "5",
        0: "a", 1: "s", 2: "d", 8: "c", 9: "v", 11: "b", 14: "e", 17: "t", 18: "1", 19: "2", 20: "3", 29: "0", 34: "i", 37: "l", 38: "j", 40: "k", 45: "n", 46: "m", 36: "enter", 43: ",", 48: "tab", 49: "space",
        51: "backspace", 53: "escape", 123: "left", 124: "right", 125: "down", 126: "up",
    ]
    let modifierNames: [CGKeyCode: String] = [0x37: "cmd", 0x38: "shift", 0x3A: "alt", 0x3B: "ctrl"]
    guard let name = names[key] else {
        throw E2EError.env("控制通道还不支持虚拟键码 \(key)，请在 keystroke(key:modifiers:) 中补充映射。")
    }
    // macOS 上 GPUI 对 ⌃⇧+数字上报“上档字符且清掉 shift 位”（US 布局 ⌃⇧3 → ctrl-#），按真实形态注入。
    let shiftedDigits: [CGKeyCode: String] = [29: ")", 19: "@", 20: "#"]
    if modifiers.sorted() == [0x38, 0x3B], let symbol = shiftedDigits[key] {
        return "ctrl-" + symbol
    }
    let prefix = try modifiers.map { code -> String in
        guard let modifier = modifierNames[code] else {
            throw E2EError.env("控制通道还不支持修饰键码 \(code)")
        }
        return modifier + "-"
    }
    return prefix.joined() + name
}

/// Characters an AppKit keyDown carries for the virtual key codes scenarios use.
func keyCharacters(_ key: CGKeyCode) throws -> String {
    let table: [CGKeyCode: String] = [
        21: "4", 23: "5",
        0: "a", 1: "s", 2: "d", 8: "c", 9: "v", 11: "b", 14: "e", 17: "t", 18: "1", 19: "2", 20: "3", 29: "0", 34: "i", 37: "l", 38: "j", 40: "k", 45: "n", 46: "m", 36: "\r", 43: ",", 48: "\t", 49: " ", 51: "\u{7f}", 53: "\u{1b}",
        123: "\u{F702}", 124: "\u{F703}", 125: "\u{F701}", 126: "\u{F700}",
    ]
    guard let value = table[key] else {
        throw E2EError.env("控制通道还不支持虚拟键码 \(key)，请在 keyCharacters 中补充映射。")
    }
    return value
}

func modifierName(_ code: CGKeyCode) throws -> String {
    let names: [CGKeyCode: String] = [0x37: "cmd", 0x38: "shift", 0x3A: "alt", 0x3B: "ctrl"]
    guard let name = names[code] else { throw E2EError.env("控制通道还不支持修饰键码 \(code)") }
    return name
}
