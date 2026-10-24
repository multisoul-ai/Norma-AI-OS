import CoreGraphics
import E2EKit
import Foundation

/// brain 模块各场景共用的步骤：建工作空间（点击偶尔没反应时重试）、关掉功能引导。
@MainActor
enum BrainSupport {
    /// 在启动页建一个叫 `name` 的工作空间：点“创建工作空间”后没弹出对话框就重点，最多三次。
    static func createWorkspace(_ session: Session, name: String) async throws {
        _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
        // 启动页刚出现时还在做入场动画，立刻点击会被吞掉。
        try await Task.sleep(nanoseconds: 2_000_000_000)
        var opened = false
        for _ in 0..<3 {
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            let frame = try await session.capture()
            if frame.contains("工作空间名称") || frame.contains("Workspace name") { opened = true; break }
            try await Task.sleep(nanoseconds: 1_500_000_000)
            if try await session.capture().contains("工作空间名称") { opened = true; break }
        }
        guard opened else {
            try? session.save(try await session.capture(), "dialog-not-opened.png")
            throw E2EError.product("三次点击后仍没有弹出创建工作空间对话框")
        }
        let dialog = try await session.capture()
        guard let input = dialog.pick(["网站改版", "Website redesign"], .bottom) else {
            throw E2EError.product("找不到工作空间名称输入框：\(dialog.summary)")
        }
        try await session.click(local: input.center, on: dialog)
        try await session.type(name)
        try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
        _ = try await session.wait(Expectation(allOf: [name], timeout: 12))
    }

    /// 设置页内容区较长：目标不在视口里时先向下再向上滚，看到后再点。
    static func clickScrolling(_ session: Session, _ label: String) async throws {
        for step in 0..<10 {
            let frame = try await session.capture()
            if let target = frame.pick([label], .bottom) {
                try await session.click(local: target.center, on: frame)
                return
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: step < 5 ? -260 : 260)
        }
        throw E2EError.product("滚动后仍找不到「\(label)」")
    }

    /// 点文字行的右端：文字识别常把同一行的几段字合成一条（如“Bella・刚刚 为什么”），
    /// 取中心会点到左边的文字，而胶囊按钮在行尾。
    static func clickEnd(_ session: Session, matching labels: [String]) async throws {
        let frame = try await session.capture()
        guard let text = frame.pick(labels, .bottom) else {
            throw E2EError.product("未识别到可点击文字：\(labels)。可见：\(frame.summary)")
        }
        try await session.click(local: CGPoint(x: text.rect.maxX - 14, y: text.rect.midY), on: frame)
    }

    /// 功能引导会盖住画面：点“跳过引导”；识别不到按钮时退回 Esc。
    static func dismissGuide(_ session: Session) async throws {
        for _ in 0..<4 {
            try await Task.sleep(nanoseconds: 1_500_000_000)
            let frame = try await session.capture()
            let showing = ["Feature Guide", "功能引导", "功能引号", "Skip Guide", "跳过引导"].contains { frame.contains($0) }
            guard showing else { return }
            if frame.contains("跳过引导") || frame.contains("Skip Guide") {
                try await session.click(matching: ["跳过引导", "Skip Guide"], pick: .bottom)
            } else {
                try await session.shortcut(key: 53, modifiers: [])
            }
        }
    }
}
