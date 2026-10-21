import E2EKit
import Fixtures
import Foundation

/// 在真实项目画布上删除第一张卡片，确认后面的卡片补进空出的格子。
@MainActor
public enum CanvasDeleteReflow {
    public nonisolated static let name = "canvas-delete-reflow"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
        }
        let workspace = "DeleteReflowCheck"

        try await session.step("建立含项目区的隔离工作空间") {
            try session.launch()
            // OCR 常把小字识别成 "Worksp3ce"，只锚定稳定片段，真实结果由数据库核对。
            _ = try await session.wait(Expectation(anyOf: ["Norma", "orkspace", "工作空间"], timeout: 25))
            try await clickPrimaryButton(session, names: ["创建工作空间", "Create Workspace"], fallbackDy: 115)
            _ = try await session.wait(Expectation(anyOf: ["Cancel", "取消", "Website", "网站改版"], timeout: 8))
            let dialog = try await session.capture()
            guard let input = dialog.pick(["网站改版", "Website redesign"], .bottom) else {
                throw E2EError.product("找不到工作空间名称输入框：\(dialog.summary)")
            }
            try await session.click(local: input.center, on: dialog)
            try await session.type(workspace)
            try await clickDialogCreate(session)
            try await Task.sleep(nanoseconds: 1_500_000_000)
            session.stop()
            guard let dataRoot = session.dataRoot else {
                throw E2EError.env("隔离数据目录丢失")
            }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            try Catalog.suppressFeatureGuide(database: dataRoot.appendingPathComponent("app.sqlite"))
            try session.launch()
            _ = try await session.wait(Expectation(anyOf: ["DeleteRef", "Reflow", "Open", "打开", "Continue"], timeout: 20))
            return try session.prove(names: [workspace], areaCount: 1, linkedTo: workspace).summary
        }

        try await session.step("进入真实项目画布") {
            let launch = try await session.capture()
            if launch.contains("搜索工作空间") || launch.contains("Search") {
                // 卡片上的 Open 按钮常被识别成 "Qpen"；只取短文本，避开 "Open workspace to load canvas" 说明文字。
                let open = launch.texts
                    .filter { ($0.value.localizedCaseInsensitiveContains("pen") || $0.value.contains("打开")) && $0.value.count <= 6 }
                    .max { $0.center.y < $1.center.y }
                guard let open else {
                    throw E2EError.product("启动页找不到工作空间卡片的 Open 按钮：\(launch.summary)")
                }
                try await session.click(local: open.center, on: launch)
            }
            _ = try await session.wait(Expectation(anyOf: ["E2E"], timeout: 12))
            try await session.click(matching: ["E2E"], pick: .bottom)
            try await Task.sleep(nanoseconds: 700_000_000)
            let project = try await session.wait(Expectation(anyOf: ["E2E", "Project"], absent: ["项目目录不可用"], timeout: 15))
            try session.save(project, "01-project-open.png")
            return "已进入项目画布"
        }

        try await session.step("排好三张终端卡片") {
            for _ in 0..<3 {
                try await runTerminalCommand(session)
            }
            let laid = try await waitForTerminalCount(session, count: 3, timeout: 8)
            try session.save(laid, "02-three-terminals.png")
            let titles = terminalTitles(laid)
            guard titles.count == 3 else {
                throw E2EError.product("宽画布应有 3 张终端，实际 \(titles.count)：\(laid.summary)")
            }
            return "三张终端已按阅读顺序排开"
        }

        try await session.step("删除第一张后其余卡片补位") {
            let before = try await session.capture()
            let titles = terminalTitles(before)
            guard titles.count == 3 else {
                throw E2EError.product("删除前应仍有 3 张终端：\(before.summary)")
            }
            // 点标题下方的卡片主体，让第一张获得焦点，再走删除卡片快捷键。
            try await session.click(
                local: CGPoint(x: titles[0].center.x + 24, y: titles[0].center.y + 90),
                on: before
            )
            try await session.shortcut(key: 2, modifiers: [0x37])
            let after = try await waitUntilReflowed(session, before: titles)
            try session.save(after, "03-after-delete.png")
            return "删除第一张后，剩下两张占到原来的第 1、2 格"
        }
    }
}

private func runTerminalCommand(_ session: Session) async throws {
    let frame = try await session.capture()
    try await session.click(
        local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
        on: frame
    )
    try await session.selectAll()
    try await session.type("/terminal")
    try await session.shortcut(key: 36, modifiers: [])
    try await Task.sleep(nanoseconds: 600_000_000)
}

/// 每张终端卡片标题栏只有一个大写 TERMINAL 徽标；Vision 有时把它和后面的
/// "Terminal" 标题粘成一段，所以按徽标计数而不是按标题全文匹配。
private func terminalTitles(_ frame: Frame) -> [VisibleText] {
    frame.texts
        .filter { text in
            // OCR 会把徽标读成 "TrRMINAL"/"TrRkllNAL"：短文本里大写字母不少于 4 个即视为徽标，
            // 以区别于只有首字母大写的 "Terminal" 标题、"E2E Project"（3 个）和较长的工作空间名。
            let upper = text.value.filter(\.isUppercase).count
            let named = text.value.contains("终端")
                || (upper >= 4 && text.value.count <= 12)
            return named
                && text.center.x > 40
                && text.center.x < frame.windowFrame.width - 24
                && text.center.y > 55
                && text.center.y < frame.windowFrame.height - 80
        }
        .sorted { lhs, rhs in
            if abs(lhs.center.y - rhs.center.y) > 48 { return lhs.center.y < rhs.center.y }
            return lhs.center.x < rhs.center.x
        }
}

@MainActor
private func waitForTerminalCount(_ session: Session, count: Int, timeout: TimeInterval) async throws -> Frame {
    let deadline = Date().addingTimeInterval(timeout)
    var last = ""
    var lastFrame: Frame?
    while Date() < deadline {
        let frame = try await session.capture()
        last = frame.summary
        lastFrame = frame
        if terminalTitles(frame).count == count { return frame }
        try await Task.sleep(nanoseconds: 200_000_000)
    }
    if let lastFrame { try session.save(lastFrame, "failure-terminal-count.png") }
    throw E2EError.product("等待 \(count) 张终端超时。可见：\(last)")
}

/// 删除后的两张标题应靠近删除前第 1、2 格，且远离原来的第 3 格。
@MainActor
private func waitUntilReflowed(_ session: Session, before: [VisibleText]) async throws -> Frame {
    let deadline = Date().addingTimeInterval(6)
    var last = ""
    var lastFrame: Frame?
    while Date() < deadline {
        let frame = try await session.capture()
        last = frame.summary
        lastFrame = frame
        let titles = terminalTitles(frame)
        if titles.count == 2,
           abs(titles[0].center.x - before[0].center.x) < 90,
           abs(titles[0].center.y - before[0].center.y) < 90,
           abs(titles[1].center.x - before[1].center.x) < 90,
           abs(titles[1].center.y - before[1].center.y) < 90,
           hypot(titles[1].center.x - before[2].center.x, titles[1].center.y - before[2].center.y) > 120 {
            return frame
        }
        try await Task.sleep(nanoseconds: 200_000_000)
    }
    if let lastFrame { try session.save(lastFrame, "failure-after-delete.png") }
    throw E2EError.product("删除第一张后卡片没有补到第 1、2 格。可见：\(last)")
}

/// 蓝底白字按钮 Vision 常漏识别：先按文字点，识别不到就点相对窗口中心的固定偏移。
private func clickPrimaryButton(_ session: Session, names: [String], fallbackDy: Double) async throws {
    let frame = try await session.capture()
    if frame.pick(names, .bottom) != nil {
        try await session.click(matching: names, pick: .bottom)
        return
    }
    try await session.click(
        local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) / 2 + fallbackDy),
        on: frame
    )
}

/// 新建工作空间对话框的确认按钮在底部右侧；识别到文字就点文字，否则点对话框右下角。
private func clickDialogCreate(_ session: Session) async throws {
    let frame = try await session.capture()
    if let button = frame.texts.filter({ $0.value.localizedCaseInsensitiveContains("Create") || $0.value.contains("创建") })
        .max(by: { $0.center.y < $1.center.y }) {
        try await session.click(local: button.center, on: frame)
        return
    }
    try await session.shortcut(key: 36, modifiers: [])
}
