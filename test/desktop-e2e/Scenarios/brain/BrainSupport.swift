import CoreGraphics
import E2EKit
import Foundation

/// brain 模块各场景共用的步骤：建工作空间（点击偶尔没反应时重试）、关掉功能引导。
@MainActor
enum BrainSupport {
    static func stateFiles(_ dir: URL, named name: String) -> [URL] {
        (FileManager.default.enumerator(at: dir, includingPropertiesForKeys: nil)?.allObjects as? [URL] ?? [])
            .filter { $0.lastPathComponent == name }
    }
    /// 在启动页建一个叫 `name` 的工作空间：点“创建工作空间”后没弹出对话框就重点，最多三次。
    static func createWorkspace(_ session: Session, name: String) async throws {
        _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
        // 启动页刚出现时还在做入场动画，立刻点击会被吞掉。
        try await Task.sleep(nanoseconds: 2_000_000_000)
        var opened = false
        for _ in 0..<3 {
            let before = try await session.capture()
            guard let button = before.pick(["创建工作空间", "Create Workspace"], .bottom) else {
                try session.save(before, "workspace-button-missing.png")
                throw E2EError.product("未识别到创建工作空间按钮：\(before.summary)")
            }
            try await session.click(local: button.center, on: before)
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
        if let input = dialog.pick(["网站改版", "Website redesign"], .bottom) {
            try await session.click(local: input.center, on: dialog)
        } else if let label = dialog.pick(["工作空间名称", "Workspace name"], .bottom) {
            // 首帧浅色占位符可能未被 OCR 识别，输入框位于当前标签下方。
            try await session.click(local: CGPoint(x: label.center.x + 110, y: label.center.y + 30), on: dialog)
        } else {
            throw E2EError.product("找不到工作空间名称输入框：\(dialog.summary)")
        }
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

    /// 从启动页进入“E2E Project”，用 `/new claude` 建 `count` 个智能体。
    static func openProjectWithAgents(_ session: Session, count: Int) async throws {
        // 以当前目录中的真实空间名进入，避免依赖旧首页占位符与固定坐标。
        guard let workspace = try session.catalog().names.first else {
            throw E2EError.product("隔离目录里没有工作空间")
        }
        try await session.shortcut(key: 53, modifiers: [])
        _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
        try await session.click(matching: [workspace], pick: .bottom)
        _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
        try await session.click(matching: ["E2E Project"], pick: .bottom)
        try await dismissGuide(session)
        _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
        try await dismissGuide(session)
        for _ in 0..<count {
            let frame = try await session.capture()
            try await session.click(
                local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
                on: frame)
            try await session.selectAll()
            try await session.type("/new claude")
            try await session.shortcut(key: 36, modifiers: [])
            try await Task.sleep(nanoseconds: 2_500_000_000)
            try await dismissGuide(session)
        }
    }

    /// ⌘⇧E 打开运行概览；快捷键偶尔被刚结束的输入吞掉，没看到“运行概览”就再按，最多三次。
    static func openOverview(_ session: Session) async throws {
        for _ in 0..<3 {
            try await session.shortcut(key: 14, modifiers: [0x37, 0x38])
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let frame = try await session.capture()
            if frame.contains("运行概览") || frame.contains("Run Overview") { return }
        }
    }

    /// 功能引导会盖住画面：点“跳过引导”；识别不到按钮时退回 Esc。
    static func dismissGuide(_ session: Session) async throws {
        for _ in 0..<4 {
            try await Task.sleep(nanoseconds: 1_500_000_000)
            let frame = try await session.capture()
            let showing = ["Feature Guide", "功能引导", "功能引号", "Skip Guide", "跳过"].contains { frame.contains($0) }
            guard showing else { return }
            if frame.contains("跳过") || frame.contains("Skip Guide") {
                try await session.click(matching: ["跳过", "Skip Guide"], pick: .bottom)
            } else {
                try await session.shortcut(key: 53, modifiers: [])
            }
        }
    }
}
