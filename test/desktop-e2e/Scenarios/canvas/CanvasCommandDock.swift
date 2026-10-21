import E2EKit
import Foundation

/// 底部 Dock 的快捷指令：输入 `/` 平铺全部叶子指令并分组，裸词直接过滤。
@MainActor
public enum CanvasCommandDock {
    public nonisolated static let name = "canvas-command-dock"

    public static func run(_ session: Session) async throws {
        var repo: URL?
        defer {
            session.stop()
            if let repo { try? FileManager.default.removeItem(at: repo) }
        }
        try await session.step("创建空工作空间") {
            // 先启动一次让 App 建好数据库，停掉后写入最近 repo，再正式启动。
            try session.launch(language: "zh-Hans")
            try await Task.sleep(nanoseconds: 4_000_000_000)
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
                .appendingPathComponent("norma-dock-repo-\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            repo = root
            try Catalog.seedRecent(
                database: dataRoot.appendingPathComponent("app.sqlite"),
                root: root,
                displayName: "dockrepo"
            )
            try session.launch(language: "zh-Hans")
            try await Task.sleep(nanoseconds: 6_000_000_000)
            let launch = try await session.capture()
            // 本机 Vision 常把中文识别成乱码，按窗口中心固定偏移点击主按钮。
            try await session.click(
                local: CGPoint(x: Double(launch.windowFrame.width) / 2, y: Double(launch.windowFrame.height) / 2 + 115),
                on: launch
            )
            try await Task.sleep(nanoseconds: 1_500_000_000)
            let dialog = try await session.capture()
            try session.save(dialog, "00-dialog.png")
            try await session.type("DockCheck")
            try await session.click(
                local: CGPoint(x: Double(dialog.windowFrame.width) / 2 + 140, y: Double(dialog.windowFrame.height) / 2 + 119),
                on: dialog
            )
            try await Task.sleep(nanoseconds: 2_500_000_000)
            try session.save(try await session.capture(), "00-canvas.png")
            return "工作空间已创建"
        }

        try await session.step("输入 / 平铺全部指令") {
            try await focusDock(session)
            try await session.type("/")
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let frame = try await session.capture()
            try session.save(frame, "01-flat-root.png")
            guard frame.contains("Codex") || frame.contains("Claude") else {
                throw E2EError.product("平铺列表未显示 Agent 行：\(frame.summary)")
            }
            // 中文标题常被 OCR 读成乱码，分组标题交给看图清单；抽屉高度封顶 372px，这里只核对前几个 Agent 叶子可见，其余靠滚动。
            guard ["Codex", "Claude", "OpenCode", "Cursor"].allSatisfy({ frame.contains($0) }) else {
                throw E2EError.product("根列表没有平铺出 Agent 叶子：\(frame.summary)")
            }
            return "根列表直接显示 Agent 叶子指令"
        }

        try await session.step("裸词 midnight 直接命中主题") {
            try await session.selectAll()
            try await session.type("midnight")
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let frame = try await session.capture()
            try session.save(frame, "02-bare-word-theme.png")
            guard frame.contains("midnight-vio"), !frame.contains("Codex") else {
                throw E2EError.product("裸词未收敛到 Midnight 主题：\(frame.summary)")
            }
            return "裸词只剩 Midnight 主题"
        }

        try await session.step("/opcode 模糊匹配并用 Tab 补全") {
            try await session.selectAll()
            try await session.type("/opcode")
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let frame = try await session.capture()
            try session.save(frame, "02b-fuzzy-agent.png")
            // 子序列命中 OpenCode，补全文本为 /new opencode；其余 Agent 不应出现。
            guard frame.contains("new opencode"), !frame.contains("Codex") else {
                throw E2EError.product("/opcode 未模糊匹配到 OpenCode：\(frame.summary)")
            }
            try await session.shortcut(key: 48, modifiers: [])
            try await Task.sleep(nanoseconds: 1_000_000_000)
            let completed = try await session.capture()
            try session.save(completed, "02c-tab-completed.png")
            guard completed.contains("new opencode") else {
                throw E2EError.product("Tab 后输入框未补全为 /new opencode：\(completed.summary)")
            }
            return "/opcode 经 Tab 补全为 /new opencode"
        }

        try await session.step("/open 同时列出浏览与 OpenCode") {
            try await session.selectAll()
            try await session.type("/open")
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let frame = try await session.capture()
            try session.save(frame, "02d-open-prefix.png")
            guard frame.contains("new opencode"), frame.contains("/open") else {
                throw E2EError.product("/open 应同时列出浏览项与 OpenCode：\(frame.summary)")
            }
            return "/open 列出浏览文件夹与新建 OpenCode"
        }

        try await session.step("/add 无匹配时回退到浏览文件夹") {
            try await session.selectAll()
            try await session.type("/add zzzz")
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let frame = try await session.capture()
            try session.save(frame, "03-add-fallback.png")
            // 名称不匹配任何最近 repo，列表只剩浏览项，其补全文本为 /open。
            guard frame.contains("open"), !frame.contains("dockrepo") else {
                throw E2EError.product("/add 未回退到浏览文件夹：\(frame.summary)")
            }
            return "/add 只剩浏览文件夹"
        }

        try await session.step("/add 名称添加最近 repo") {
            try await session.selectAll()
            try await session.type("/add dock")
            try await Task.sleep(nanoseconds: 1_200_000_000)
            let frame = try await session.capture()
            try session.save(frame, "04-add-recent.png")
            guard frame.contains("add dockrepo"), frame.contains("open") else {
                throw E2EError.product("/add dock 应列出 dockrepo 与浏览项：\(frame.summary)")
            }
            try await session.shortcut(key: 36, modifiers: [])
            let deadline = Date().addingTimeInterval(10)
            var last = "未检查"
            while Date() < deadline {
                let catalog = try session.catalog()
                if let area = catalog.areas.first, let repo, area.rootPath.hasSuffix(repo.lastPathComponent) {
                    try await Task.sleep(nanoseconds: 1_500_000_000)
                    let after = try await session.capture()
                    try session.save(after, "05-after-add.png")
                    // 抽屉应已收起，不再显示候选的 /add 补全文本。
                    guard !after.contains("add dockrepo") else {
                        throw E2EError.product("添加后抽屉仍显示候选：\(after.summary)")
                    }
                    return "项目区已写入 \(area.rootPath)"
                }
                last = catalog.summary
                try await Task.sleep(nanoseconds: 300_000_000)
            }
            throw E2EError.product("回车后项目区没有出现 dockrepo。\(last)")
        }
    }
}

private func focusDock(_ session: Session) async throws {
    let frame = try await session.capture()
    try await session.click(
        local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
        on: frame
    )
    try await session.selectAll()
}
