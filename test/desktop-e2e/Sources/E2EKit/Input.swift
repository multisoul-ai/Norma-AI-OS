import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

extension Session {
    /// 尽量把隔离窗口抬到最前。点击不依赖 isActive；键盘在仍未获得焦点时会取消发送。
    public func activate() async throws {
        try ensureRunning()
        _ = await becomeFrontmost()
    }

    private func becomeFrontmost() async -> Bool {
        guard let pid = app?.processIdentifier,
              let running = NSRunningApplication(processIdentifier: pid) else { return false }
        for _ in 1...4 {
            _ = running.activate(from: NSRunningApplication.current, options: [.activateAllWindows])
            raiseViaAccessibility(pid)
            if running.isActive { return true }
            try? await Task.sleep(nanoseconds: 180_000_000)
        }
        return running.isActive
    }

    private func requireKeyboardFocus() async throws {
        if await becomeFrontmost() { return }
        throw E2EError.product("隔离 Norma 没有键盘焦点，已取消按键，避免输入进其他应用。")
    }

    public func click(matching candidates: [String], pick: TextPick) async throws {
        try await activate()
        let frame = try await capture()
        guard let text = frame.pick(candidates, pick) else {
            throw E2EError.product("未识别到可点击文字：\(candidates.joined(separator: " / "))。可见：\(frame.summary)")
        }
        try await click(local: text.center, on: frame, requery: candidates, pick: pick)
    }

    public func click(local: CGPoint, on frame: Frame) async throws {
        try await click(local: local, on: frame, requery: nil, pick: nil)
    }

    private func click(local: CGPoint, on frame: Frame, requery: [String]?, pick: TextPick?) async throws {
        try await activate()
        var point = local
        var window = try await currentWindow()
        if window.id != frame.windowID || abs(window.frame.width - frame.windowFrame.width) > 2 {
            if let requery, let pick {
                let fresh = try await capture()
                guard let text = fresh.pick(requery, pick) else {
                    throw E2EError.product("重取窗口后未重新识别：\(requery.joined(separator: " / "))")
                }
                point = text.center
                window = (fresh.windowID, fresh.windowFrame)
            }
        }
        var global = CGPoint(x: window.frame.minX + point.x, y: window.frame.minY + point.y)
        try postMove(global)
        try await Task.sleep(nanoseconds: 160_000_000)
        let moved = try await currentWindow()
        if abs(moved.frame.minX - window.frame.minX) > 2 || abs(moved.frame.minY - window.frame.minY) > 2 {
            if let requery, let pick {
                let fresh = try await capture()
                guard let text = fresh.pick(requery, pick) else {
                    throw E2EError.product("窗口移动后未重新识别：\(requery.joined(separator: " / "))")
                }
                point = text.center
                global = CGPoint(x: fresh.windowFrame.minX + point.x, y: fresh.windowFrame.minY + point.y)
            } else {
                global = CGPoint(x: moved.frame.minX + point.x, y: moved.frame.minY + point.y)
            }
            try postMove(global)
            try await Task.sleep(nanoseconds: 80_000_000)
        }
        try await requireOwnWindow(at: global)
        try postClick(global)
    }

    private func requireOwnWindow(at global: CGPoint) async throws {
        if ownsScreenPoint(global) { return }
        _ = await becomeFrontmost()
        try await Task.sleep(nanoseconds: 200_000_000)
        guard ownsScreenPoint(global) else {
            throw E2EError.product("点击点被其他窗口挡住，已取消这次点击，避免点到别的 Norma。")
        }
    }

    private func ownsScreenPoint(_ global: CGPoint) -> Bool {
        guard let pid = app?.processIdentifier,
              let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else {
            return false
        }
        for window in windows {
            let layer = window[kCGWindowLayer as String] as? Int ?? 0
            let alpha = window[kCGWindowAlpha as String] as? Double ?? 1
            if layer != 0 || alpha < 0.05 { continue }
            guard let bounds = window[kCGWindowBounds as String] as? NSDictionary,
                  let rect = CGRect(dictionaryRepresentation: bounds),
                  rect.width > 2, rect.height > 2, rect.contains(global) else { continue }
            let owner = window[kCGWindowOwnerPID as String] as? Int32
                ?? (window[kCGWindowOwnerPID as String] as? Int).map(Int32.init)
            return owner == pid
        }
        return false
    }

    private func raiseViaAccessibility(_ pid: pid_t) {
        let application = AXUIElementCreateApplication(pid)
        AXUIElementSetAttributeValue(application, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
        var raw: CFTypeRef?
        guard AXUIElementCopyAttributeValue(application, kAXWindowsAttribute as CFString, &raw) == .success else { return }
        let windows = raw as? [AXUIElement] ?? []
        for window in windows {
            AXUIElementPerformAction(window, kAXRaiseAction as CFString)
        }
    }

    public func type(_ value: String) async throws {
        try await requireKeyboardFocus()
        let source = CGEventSource(stateID: .hidSystemState)
        for scalar in value.utf16 {
            guard let down = CGEvent(keyboardEventSource: source, virtualKey: 0, keyDown: true),
                  let up = CGEvent(keyboardEventSource: source, virtualKey: 0, keyDown: false) else {
                throw E2EError.env("无法创建文字输入事件")
            }
            var character = scalar
            down.keyboardSetUnicodeString(stringLength: 1, unicodeString: &character)
            up.keyboardSetUnicodeString(stringLength: 1, unicodeString: &character)
            down.post(tap: .cghidEventTap)
            try await Task.sleep(nanoseconds: 16_000_000)
            up.post(tap: .cghidEventTap)
            try await Task.sleep(nanoseconds: 18_000_000)
        }
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

    private func hotkey(key: CGKeyCode, modifiers: [CGKeyCode]) async throws {
        try await requireKeyboardFocus()
        let source = CGEventSource(stateID: .hidSystemState)
        func event(_ virtualKey: CGKeyCode, down: Bool, flags: CGEventFlags) throws -> CGEvent {
            guard let event = CGEvent(keyboardEventSource: source, virtualKey: virtualKey, keyDown: down) else {
                throw E2EError.env("无法创建键盘事件")
            }
            event.flags = flags
            return event
        }
        var flags = CGEventFlags()
        for modifier in modifiers {
            if modifier == 0x37 { flags.insert(.maskCommand) }
            if modifier == 0x38 { flags.insert(.maskShift) }
            if modifier == 0x3A { flags.insert(.maskAlternate) }
            if modifier == 0x3B { flags.insert(.maskControl) }
        }
        for modifier in modifiers {
            try event(modifier, down: true, flags: flags).post(tap: .cghidEventTap)
        }
        try event(key, down: true, flags: flags).post(tap: .cghidEventTap)
        try await Task.sleep(nanoseconds: 30_000_000)
        try event(key, down: false, flags: flags).post(tap: .cghidEventTap)
        for modifier in modifiers.reversed() {
            try event(modifier, down: false, flags: CGEventFlags()).post(tap: .cghidEventTap)
        }
    }

    private func postMove(_ global: CGPoint) throws {
        guard let event = CGEvent(
            mouseEventSource: nil,
            mouseType: .mouseMoved,
            mouseCursorPosition: global,
            mouseButton: .left
        ) else { throw E2EError.env("无法创建鼠标移动事件") }
        event.post(tap: .cghidEventTap)
    }

    private func postClick(_ global: CGPoint) throws {
        guard let down = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: global, mouseButton: .left),
              let up = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: global, mouseButton: .left) else {
            throw E2EError.env("无法创建鼠标点击事件")
        }
        down.setIntegerValueField(.mouseEventClickState, value: 1)
        up.setIntegerValueField(.mouseEventClickState, value: 1)
        down.post(tap: .cghidEventTap)
        Thread.sleep(forTimeInterval: 0.04)
        up.post(tap: .cghidEventTap)
    }
}
