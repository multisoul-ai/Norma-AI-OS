import AppKit
import E2EKit
import Foundation

/// 在真实桌面窗口中验收左右抽屉及中央画布占位，只经 debug 测试控制通道发送原始输入，不读取 App 内部状态。
@MainActor
public enum CanvasDrawers {
    public nonisolated static let name = "canvas-drawers"

    public static func run(_ session: Session) async throws {
        defer { session.stop() }
        try await session.step("创建抽屉验收工作空间") {
            try session.launch(language: "zh-Hans")
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(anyOf: ["工作空间名称", "Workspace name"], timeout: 4))
            let dialog = try await session.capture()
            guard let placeholder = dialog.pick(["网站改版", "Website redesign"], .bottom) else {
                throw E2EError.product("创建对话框缺少名称输入框。可见：\(dialog.summary)")
            }
            try await session.click(local: placeholder.center, on: dialog)
            try await session.type("DrawerCheck")
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            let collapsed = try await waitFor(session) { frame in
                !leftVisible(frame) && canvasTitle(frame) != nil && wallpaperVisible(frame)
            }
            try assertCanvasRegion(collapsed, leftOpen: false, rightOpen: false)
            try session.save(collapsed, "00-left-default-closed.png")
            try await clickReopen(session, side: .left)
            let frame = try await waitFor(session) { frame in
                leftVisible(frame) && canvasTitle(frame) != nil && wallpaperVisible(frame)
            }
            try assertCanvasRegion(frame, leftOpen: true, rightOpen: false)
            // 场景：右抽屉收起时入口只保留图标，避免标题占据画布顶栏。
            // 数据构造：1280px 宽桌面窗口，左抽屉展开，右抽屉保持收起。
            // 执行过程：等待工作空间画布稳定后检查顶栏可见文字。
            // 预期结果：右栏正文不可见，顶栏也没有“内容浏览器”文字。
            guard !frame.contains("内容浏览器") else {
                throw E2EError.product("右抽屉图标入口仍显示冗余文字：\(frame.summary)")
            }
            try session.save(frame, "01-left-open.png")
            // 场景：Norma 光环应缓慢流动，不应停留在原来的三层静态边框。
            // 数据构造：默认壁纸和空工作空间保持不变，仅让光环运行约一秒。
            // 执行过程：保留初帧并在下一段运动后再次截取真实窗口。
            // 预期结果：两张图供视觉检查确认柔光高亮沿单圈移动。
            try await Task.sleep(nanoseconds: 1_100_000_000)
            try session.save(try await session.capture(), "01-orb-later.png")
            let catalog = try session.prove(names: ["DrawerCheck"], areaCount: 0)
            return "\(catalog.summary) canvasCenter=\(try centerOfCanvas(frame))"
        }

        try await session.step("左抽屉滑出") {
            try await clickTopClose(session, side: .left)
            _ = try await waitFor(session) { frame in
                !leftVisible(frame) && canvasTitle(frame) != nil
            }
            let frame = try await settledFrame(session)
            guard !leftVisible(frame) else {
                throw E2EError.product("左抽屉停稳后仍有面板内容可见。可见：\(frame.summary)")
            }
            try assertCanvasRegion(frame, leftOpen: false, rightOpen: false)
            try session.save(frame, "02-left-closed.png")
            return "左栏已退出，中央画布扩展至左边缘"
        }

        try await session.step("右抽屉滑入且左栏保持收起") {
            try await clickReopen(session, side: .right)
            _ = try await waitFor(session) { frame in
                rightVisible(frame)
                    && !leftVisible(frame)
            }
            let frame = try await settledFrame(session)
            try assertCanvasRegion(frame, leftOpen: false, rightOpen: true)
            try session.save(frame, "03-right-open.png")
            return "右栏内容可读，左栏仍收起"
        }

        try await session.step("左右抽屉同时展开") {
            try await clickReopen(session, side: .left)
            _ = try await waitFor(session) { frame in
                leftVisible(frame) && rightVisible(frame)
            }
            let opened = try await settledFrame(session)
            try assertCanvasRegion(opened, leftOpen: true, rightOpen: true)
            // 场景：双抽屉展开后原型的右栏标题区只保留收起按钮与内容标签。
            // 数据构造：空内容工作空间，左右抽屉都在 1280px 桌面窗口完整展开。
            // 执行过程：等待滑动终态，再检查右栏标题区的可见文字。
            // 预期结果：空态正文仍可读，但不额外显示“内容浏览器”标题挤占窄画布。
            guard !opened.contains("内容浏览器") else {
                throw E2EError.product("双抽屉展开时右栏多出原型没有的标题：\(opened.summary)")
            }
            try session.save(opened, "04-both-open.png")
            return "两侧同时展开，中间画布占据剩余空间"
        }

        try await session.step("右抽屉滑出且左栏保持展开") {
            try await clickTopClose(session, side: .right)
            _ = try await waitFor(session) { frame in
                leftVisible(frame)
                    && !rightVisible(frame)
            }
            let frame = try await settledFrame(session)
            guard !rightVisible(frame) else {
                throw E2EError.product("右抽屉停稳后仍有面板内容可见。可见：\(frame.summary)")
            }
            try assertCanvasRegion(frame, leftOpen: true, rightOpen: false)
            try session.save(frame, "05-right-closed.png")
            return "右栏已退出，中央画布扩展至右边缘"
        }

        // 旧「悬浮运行概览」已被左栏「对话 / 运行概览」分段取代，覆盖见 workspace-drawer-tabs 与 workspace-overview-float。

        try await session.step("左侧对话框可输入") {
            let frame = try await session.capture()
            try await session.click(
                local: CGPoint(x: 120, y: Double(frame.windowFrame.height) - 38),
                on: frame
            )
            try await session.type("抽屉输入验收")
            let typed = try await waitFor(session) { $0.contains("抽屉输入") }
            guard !typed.contains("发送给 Norma") else {
                throw E2EError.product("左栏输入弹出了中央 Dock 建议面板：\(typed.summary)")
            }
            try session.save(typed, "08-chat-input.png")
            return "左栏输入与现有 Norma 命令输入共享会话状态"
        }

        try await session.step("拖动左栏边缘扩大画布占位") {
            let before = try await session.capture()
            let width = Double(before.windowFrame.width)
            let edge = min(438, max(366, width * 0.31))
            let oldCenter = try centerOfCanvas(before)
            try await session.drag(
                local: CGPoint(x: edge, y: 260),
                to: CGPoint(x: edge + 100, y: 260),
                on: before
            )
            let resized = try await settledFrame(session)
            guard leftVisible(resized),
                  abs((try centerOfCanvas(resized)) - oldCenter - 50) <= 18 else {
                throw E2EError.product("拖大左栏后中央画布未同步缩窄：\(resized.summary)")
            }
            try session.save(resized, "09-left-resized.png")
            return "左栏增加约 100px，中央画布边缘同步移动"
        }

        try await session.step("左栏拖过最小宽度自动收起") {
            let before = try await session.capture()
            let width = Double(before.windowFrame.width)
            let edge = min(438, max(366, width * 0.31)) + 100
            try await session.drag(
                local: CGPoint(x: edge, y: 260),
                to: CGPoint(x: 100, y: 260),
                on: before
            )
            _ = try await waitFor(session) { !leftVisible($0) }
            let collapsed = try await settledFrame(session)
            guard abs((try centerOfCanvas(collapsed)) - width / 2) <= 18 else {
                throw E2EError.product("左栏收起后画布未回收空间：\(collapsed.summary)")
            }
            try session.save(collapsed, "10-left-drag-collapsed.png")
            return "左栏低于 150px 后收起"
        }

        try await session.step("拖动右栏边缘扩大并收起") {
            try await clickReopen(session, side: .right)
            _ = try await waitFor(session) { rightVisible($0) }
            let before = try await settledFrame(session)
            let width = Double(before.windowFrame.width)
            let edge = width - min(560, max(370, width * 0.37))
            let oldCenter = try centerOfCanvas(before)
            try await session.drag(
                local: CGPoint(x: edge, y: 260),
                to: CGPoint(x: edge - 80, y: 260),
                on: before
            )
            let resized = try await settledFrame(session)
            guard rightVisible(resized),
                  abs((try centerOfCanvas(resized)) - oldCenter + 40) <= 18 else {
                throw E2EError.product("拖大右栏后中央画布未同步缩窄：\(resized.summary)")
            }
            try session.save(resized, "11-right-resized.png")
            try await session.drag(
                local: CGPoint(x: edge - 82, y: 260),
                to: CGPoint(x: width - 10, y: 260),
                on: resized
            )
            try session.save(try await session.capture(), "12-right-after-drag.png")
            _ = try await waitFor(session) { !rightVisible($0) }
            let collapsed = try await settledFrame(session)
            guard abs((try centerOfCanvas(collapsed)) - width / 2) <= 18 else {
                throw E2EError.product("右栏收起后画布未回收空间：\(collapsed.summary)")
            }
            try session.save(collapsed, "12-right-drag-collapsed.png")
            return "右栏低于 185px 后收起，画布恢复全宽"
        }

        try await session.step("拖拽结束后右栏入口仍可操作") {
            try await clickReopen(session, side: .right)
            _ = try await waitFor(session) { rightVisible($0) }
            let reopened = try await settledFrame(session)
            let width = Double(reopened.windowFrame.width)
            guard abs((try centerOfCanvas(reopened)) - (width - 185) / 2) <= 18 else {
                throw E2EError.product("重新打开右栏后未恢复最小宽度：\(reopened.summary)")
            }
            try session.save(reopened, "13-right-reopened.png")
            return "拖拽捕获已释放，右栏可以通过图标重新展开"
        }
    }
}

private enum DrawerSide { case left, right }

@MainActor
private func clickTopClose(_ session: Session, side: DrawerSide) async throws {
    let frame = try await session.capture()
    if side == .left && !leftVisible(frame) || side == .right && !rightVisible(frame) {
        throw E2EError.product("\(side == .left ? "左" : "右")抽屉未展开。可见：\(frame.summary)")
    }
    let width = Double(frame.windowFrame.width)
    let rightWidth = min(560, max(370, width * 0.37))
    let leftWidth = min(438, max(366, width * 0.31))
    // 左侧开关是标题栏固定锚点（x≈98），展开与收起位置一致；右侧开关在右抽屉内。
    let x = side == .left ? 98 : width - rightWidth + 28
    try await session.click(local: CGPoint(x: x, y: 26), on: frame)
}

@MainActor
private func clickReopen(_ session: Session, side: DrawerSide) async throws {
    let frame = try await session.capture()
    let x = side == .left ? 98 : Double(frame.windowFrame.width) - 28
    try await session.click(local: CGPoint(x: x, y: 26), on: frame)
}

@MainActor
private func waitFor(
    _ session: Session,
    timeout: TimeInterval = 6,
    predicate: (Frame) -> Bool
) async throws -> Frame {
    let deadline = Date().addingTimeInterval(timeout)
    var last = ""
    while Date() < deadline {
        let frame = try await session.capture()
        last = frame.summary
        if predicate(frame) { return frame }
        try await Task.sleep(nanoseconds: 100_000_000)
    }
    throw E2EError.product("抽屉未到达预期状态。可见：\(last)")
}

@MainActor
private func settledFrame(_ session: Session) async throws -> Frame {
    // 关闭动画最长 160ms；留两帧余量后再保存终态，避免把过渡帧当成验收截图。
    try await Task.sleep(nanoseconds: 300_000_000)
    return try await session.capture()
}

private func centerOfCanvas(_ frame: Frame) throws -> Double {
    guard let title = canvasTitle(frame) else {
        throw E2EError.product("未找到中央画布标题。可见：\(frame.summary)")
    }
    return Double(title.center.x)
}

private func canvasTitle(_ frame: Frame) -> VisibleText? {
    frame.pick(["承载项目的画布", "承载项目画布", "Project canvas"], .bottom)
}

private func leftVisible(_ frame: Frame) -> Bool {
    // 大字有时被 Vision 拆段并吞掉空格；短标签是稳定的左栏独有锚点。
    frame.contains("当前对话")
}

private func rightVisible(_ frame: Frame) -> Bool {
    frame.contains("搜索或输入网址")
}

private func wallpaperVisible(_ frame: Frame) -> Bool {
    // 空工作空间的中心右下方不被卡片遮挡；壁纸加载后这里是明亮地平线。
    let bitmap = NSBitmapImageRep(cgImage: frame.image)
    let x = Int(Double(bitmap.pixelsWide) * 0.60)
    let y = Int(Double(bitmap.pixelsHigh) * 0.53)
    guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.deviceRGB) else {
        return false
    }
    return (color.redComponent + color.greenComponent + color.blueComponent) / 3 > 0.30
}

private func assertCanvasRegion(_ frame: Frame, leftOpen: Bool, rightOpen: Bool) throws {
    let current = try centerOfCanvas(frame)
    let width = Double(frame.windowFrame.width)
    let left = leftOpen ? min(438, max(366, width * 0.31)) : 0
    let right = rightOpen ? min(560, max(370, width * 0.37)) : 0
    let expected = left + (width - left - right) / 2
    guard abs(current - expected) <= 18 else {
        throw E2EError.product("画布没有按抽屉占位布局：预期中心 \(expected)，实际 \(current)，左开 \(leftOpen)，右开 \(rightOpen)")
    }
}
