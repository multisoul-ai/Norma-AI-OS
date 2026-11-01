import E2EKit
import Fixtures
import Foundation

/// 原生 Codex 握手期间弹出交互菜单（如「Update available」）：键盘必须可达，卡死仍会按 60 秒超时退出。
/// 假 codex 见 `FakeCodexUpdateMenu`；握手本身（真实中继协议）由 runtime 的 Rust 集成测试覆盖。
@MainActor
public enum AgentCodexUpdateMenu {
    public nonisolated static let name = "agent-codex-update-menu"

    public static func run(_ session: Session) async throws {
        var projectRoot: URL?
        var fake: FakeCodexUpdateMenu.Install?
        defer {
            session.stop()
            if let projectRoot { try? FileManager.default.removeItem(at: projectRoot) }
            if let fake { try? FileManager.default.removeItem(at: fake.root) }
        }
        let workspace = "CodexMenuCheck"

        try await session.step("建立隔离工作空间与项目") {
            let install = try FakeCodexUpdateMenu.install()
            fake = install
            setenv("ZDOTDIR", install.root.path, 1)
            setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
            try session.launch(homeDirectory: install.home)
            _ = try await session.wait(Expectation(anyOf: ["创建工作空间", "Create Workspace"], timeout: 25))
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(anyOf: ["工作空间名称", "Workspace name"], timeout: 4))
            let dialog = try await session.capture()
            guard let input = dialog.pick(["网站改版", "Website redesign"], .bottom) else {
                throw E2EError.product("找不到工作空间名称输入框：\(dialog.summary)")
            }
            try await session.click(local: input.center, on: dialog)
            try await session.type(workspace)
            try await session.click(matching: ["创建工作空间", "Create Workspace"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 12))
            session.stop()
            guard let dataRoot = session.dataRoot else { throw E2EError.env("隔离数据目录丢失") }
            projectRoot = try CanvasProjectFixture.install(dataRoot: dataRoot, workspaceName: workspace)
            try Catalog.suppressFeatureGuide(database: dataRoot.appendingPathComponent("app.sqlite"))
            try session.launch(homeDirectory: install.home)
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 20))
            return try session.prove(names: [workspace], areaCount: 1, linkedTo: workspace).summary
        }

        try await session.step("进入项目") {
            // 首页是工作空间卡片，点击标题进入。
            _ = try await session.wait(Expectation(allOf: [workspace], timeout: 10))
            // 冷启动偶发：残留输入唤起首页搜索浮层，先 Esc 关掉再点卡片。
            if try await session.capture().contains("输入名字就行") {
                try await session.shortcut(key: 53, modifiers: [])
                try await Task.sleep(nanoseconds: 600_000_000)
            }
            try await session.click(matching: ["1个项目", "1 project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], timeout: 12))
            try await session.click(matching: ["E2E Project"], pick: .bottom)
            _ = try await session.wait(Expectation(allOf: ["E2E Project"], absent: ["项目目录不可用"], timeout: 15))
            try await Task.sleep(nanoseconds: 900_000_000)
            let frame = try await session.capture()
            if frame.contains("功能引导") || frame.contains("Feature Guide") {
                try await session.shortcut(key: 53, modifiers: [])
                try await Task.sleep(nanoseconds: 600_000_000)
            }
            return try await session.capture().summary
        }

        try await session.step("握手期间菜单上下键移动选择，回车确认") {
            try await newCodex(session)
            let menu = try await session.wait(Expectation(allOf: ["UPDATE MENU", "SELECTED 1"], timeout: 20))
            try session.save(menu, "01-menu-shown.png")
            // 点击卡片正文获得键盘焦点（指令栏创建卡片后仍持有焦点）。
            try await session.click(matching: ["UPDATE MENU"], pick: .bottom)
            try await Task.sleep(nanoseconds: 500_000_000)
            try await session.shortcut(key: 125, modifiers: [])
            let down = try await session.wait(Expectation(allOf: ["SELECTED 2"], timeout: 6))
            try session.save(down, "02-menu-down.png")
            try await session.shortcut(key: 126, modifiers: [])
            _ = try await session.wait(Expectation(allOf: ["SELECTED 1"], absent: ["SELECTED 2"], timeout: 6))
            try await session.shortcut(key: 125, modifiers: [])
            _ = try await session.wait(Expectation(allOf: ["SELECTED 2"], timeout: 6))
            try await session.shortcut(key: 36, modifiers: [])
            let chosen = try await session.wait(Expectation(allOf: ["CHOSEN 2"], absent: ["Codex 启动失败"], timeout: 6))
            try session.save(chosen, "03-menu-chosen.png")
            return "↓/↑ 移动选择，回车确认后卡片仍存活"
        }

        try await session.step("真正卡死（刷屏不连接）60 秒后报超时并退出") {
            guard let fake else { throw E2EError.env("假 CLI 目录丢失") }
            try fake.setMode("stuck")
            try await newCodex(session)
            _ = try await session.wait(Expectation(allOf: ["STUCK TICK"], timeout: 20))
            let early = try await session.capture()
            guard !early.contains("Codex 启动失败") else {
                throw E2EError.product("刷屏几秒就被判超时，空闲预算过短：\(early.summary)")
            }
            let failed = try await session.wait(Expectation(allOf: ["Codex 启动失败", "进程已退出"], timeout: 90))
            try session.save(failed, "04-stuck-timed-out.png")
            return "持续输出不续命，超时后报错并退出"
        }
    }

    private static func newCodex(_ session: Session) async throws {
        let frame = try await session.capture()
        try await session.click(
            local: CGPoint(x: Double(frame.windowFrame.width) / 2, y: Double(frame.windowFrame.height) - 38),
            on: frame
        )
        try await session.selectAll()
        try await session.type("/new codex")
        try await session.shortcut(key: 36, modifiers: [])
        try await Task.sleep(nanoseconds: 1_500_000_000)
    }
}
