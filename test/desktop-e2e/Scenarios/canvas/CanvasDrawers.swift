import AppKit
import E2EKit
import Foundation

/// 在真实桌面窗口中验收右抽屉及中央画布占位，只经 debug 测试控制通道发送原始输入，不读取 App 内部状态。
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
                canvasTitle(frame) != nil && wallpaperVisible(frame)
            }
            try assertCanvasRegion(collapsed, rightOpen: false)
            try session.save(collapsed, "00-right-default-closed.png")
            let catalog = try session.prove(names: ["DrawerCheck"], areaCount: 0)
            return "\(catalog.summary) canvasCenter=\(try centerOfCanvas(collapsed))"
        }

        try await session.step("右抽屉滑入") {
            try await clickReopen(session)
            _ = try await waitFor(session) { rightVisible($0) }
            let frame = try await settledFrame(session)
            try assertCanvasRegion(frame, rightOpen: true)
            try session.save(frame, "03-right-open.png")
            return "右栏内容可读，画布让出右侧占位"
        }

        try await session.step("右抽屉滑出") {
            try await clickTopClose(session)
            _ = try await waitFor(session) { !rightVisible($0) }
            let frame = try await settledFrame(session)
            try assertCanvasRegion(frame, rightOpen: false)
            try session.save(frame, "05-right-closed.png")
            return "右栏已退出，中央画布扩展至右边缘"
        }

        try await session.step("拖动右栏边缘扩大并收起") {
            try await clickReopen(session)
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
            try await clickReopen(session)
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

@MainActor
private func clickTopClose(_ session: Session) async throws {
    let frame = try await session.capture()
    guard rightVisible(frame) else {
        throw E2EError.product("右抽屉未展开。可见：\(frame.summary)")
    }
    let width = Double(frame.windowFrame.width)
    // 右侧开关在右抽屉内。
    try await session.click(local: CGPoint(x: width - min(560, max(370, width * 0.37)) + 28, y: 26), on: frame)
}

@MainActor
private func clickReopen(_ session: Session) async throws {
    let frame = try await session.capture()
    try await session.click(local: CGPoint(x: Double(frame.windowFrame.width) - 28, y: 26), on: frame)
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

private func rightVisible(_ frame: Frame) -> Bool {
    // 空态右栏的稳定锚点；OCR 偶尔漏读其中一个，任一出现即视为展开。
    ["搜索或输入网址", "推荐", "更多工具"].contains { frame.contains($0) }
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

private func assertCanvasRegion(_ frame: Frame, rightOpen: Bool) throws {
    let current = try centerOfCanvas(frame)
    let width = Double(frame.windowFrame.width)
    let right = rightOpen ? min(560, max(370, width * 0.37)) : 0
    let expected = (width - right) / 2
    guard abs(current - expected) <= 18 else {
        throw E2EError.product("画布没有按抽屉占位布局：预期中心 \(expected)，实际 \(current)，右开 \(rightOpen)")
    }
}
