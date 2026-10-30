import CoreGraphics
import E2EKit
import Foundation

/// Grok 编排安装/卸载的真实窗口验收，仅写隔离子进程 home。
@MainActor
public enum SettingsGrokOrchestration {
    public nonisolated static let name = "settings-grok-orchestration"

    public static func run(_ session: Session) async throws {
        guard let output = session.output else { throw E2EError.env("缺少产物目录") }
        let home = output.appendingPathComponent("grok-home", isDirectory: true)
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        defer { session.stop() }
        try session.launch(language: "en", homeDirectory: home)
        _ = try await session.wait(Expectation(anyOf: ["Create Workspace", "创建工作空间"], timeout: 25))
        session.stop()
        let database = try session.databaseURL()
        try Catalog.suppressFeatureGuide(database: database)
        for (locale, theme, tag) in [("en", "graphite", "en-graphite"), ("zh-CN", "sapphire-night", "zh-sapphire")] {
            try Catalog.seedSetting(database: database, key: "general.language.v1", value: locale)
            try Catalog.seedSetting(database: database, key: "appearance.theme.v1", value: theme)
            try await session.step("\(tag)：Grok 可安装且标明实验性") {
                try session.launch(language: locale, homeDirectory: home)
                _ = try await session.wait(Expectation(anyOf: ["Create Workspace", "创建工作空间"], timeout: 25))
                try await session.shortcut(key: 43, modifiers: [0x37])
                _ = try await session.wait(Expectation(anyOf: ["Settings", "设置"], timeout: 10))
                try session.save(try await session.capture(), "\(tag)-00-open.png")
                do {
                    _ = try await SettingsSupport.select(session, labels: ["Orchestration", "编排"], content: ["Grok Build"])
                } catch {
                    // 深色中文的短标签可能被 OCR 漏掉。已看图确认编排行紧邻手机连接上方，行距 44pt。
                    let frame = try await session.capture()
                    guard let mobile = frame.pick(["手机连接"], .left),
                          mobile.center.x < frame.windowFrame.width * 0.4 else {
                        try session.save(frame, "\(tag)-navigation-failed.png")
                        throw error
                    }
                    try await session.click(local: CGPoint(x: mobile.center.x, y: mobile.center.y - 44), on: frame)
                    _ = try await session.wait(Expectation(anyOf: ["安装 Skill", "文件路径输出约定"], timeout: 5))
                }
                let frame = try await row(session)
                try require(frame, anyOf: ["Not installed", "未安装"])
                try require(frame, anyOf: ["Experimental", "实验性"])
                try session.save(frame, "\(tag)-01-installable.png")
                try await action(session, labels: ["Install", "安装"], hover: true)
                try session.save(try await session.capture(), "\(tag)-02-hover.png")
                return "Grok 行显示未安装与实验性提示，安装按钮可交互"
            }
            try await session.step("\(tag)：安装后四组件就绪并写入原生目录") {
                try await action(session, labels: ["Install", "安装"])
                let frame = try await waitRow(session, anyOf: ["Ready", "就绪"])
                for name in ["norma-orchestration", "norma-presentation", "ask-norma"] {
                    let file = home.appendingPathComponent(".grok/skills/\(name)/SKILL.md")
                    guard FileManager.default.fileExists(atPath: file.path) else {
                        throw E2EError.product("安装后缺少 \(name) 技能")
                    }
                }
                let hook = home.appendingPathComponent(".grok/hooks/multisoul-agent-hook.json")
                guard try String(contentsOf: hook, encoding: .utf8).contains("forward --source grok") else {
                    throw E2EError.product("Grok 原生 Hook 未安装")
                }
                let credential = home.appendingPathComponent("Library/Application Support/Norma OS/orchestration/grok.json")
                guard FileManager.default.fileExists(atPath: credential.path) else {
                    throw E2EError.product("Grok 凭据未写入")
                }
                try session.save(frame, "\(tag)-03-ready.png")
                return "真实安装回调显示 Ready，三个技能、Hook 和凭据已落盘"
            }
            try await session.step("\(tag)：确认卸载后恢复未安装且保留用户文件") {
                let note = home.appendingPathComponent(".grok/skills/norma-orchestration/user-note.txt")
                try Data("keep user file".utf8).write(to: note)
                try await action(session, labels: ["Uninstall", "卸载"])
                _ = try await waitRow(session, anyOf: ["Confirm", "确认", "只会移除由 Norma"])
                try session.save(try await session.capture(), "\(tag)-04-confirm.png")
                try await action(session, labels: ["Confirm", "确认"])
                let frame = try await waitRow(session, anyOf: ["Not installed", "未安装"])
                let skill = home.appendingPathComponent(".grok/skills/norma-orchestration/SKILL.md")
                let credential = home.appendingPathComponent("Library/Application Support/Norma OS/orchestration/grok.json")
                guard !FileManager.default.fileExists(atPath: skill.path),
                      !FileManager.default.fileExists(atPath: credential.path),
                      FileManager.default.fileExists(atPath: note.path) else {
                    throw E2EError.product("卸载应删除受管技能及凭据，同时保留用户文件")
                }
                try session.save(frame, "\(tag)-05-uninstalled.png")
                session.stop()
                return "卸载完成且用户笔记保留"
            }
        }
    }

    private static func row(_ session: Session) async throws -> Frame {
        for _ in 0..<10 {
            let frame = try await session.capture()
            if let title = frame.pick(["Grok Build"], .right),
               title.center.y < frame.windowFrame.height - 180 { return frame }
            try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: -120)
        }
        throw E2EError.product("无法滚动到完整 Grok Build 行")
    }

    private static func rowText(_ frame: Frame) throws -> [VisibleText] {
        guard let title = frame.pick(["Grok Build"], .right) else { throw E2EError.product("缺少 Grok 标题") }
        return frame.texts.filter { $0.center.x >= title.center.x - 60
            && $0.center.y >= title.center.y - 12 && $0.center.y < title.center.y + 62 }
    }

    private static func require(_ frame: Frame, anyOf labels: [String]) throws {
        let texts = try rowText(frame)
        guard texts.contains(where: { text in labels.contains { text.value.localizedCaseInsensitiveContains($0) } }) else {
            throw E2EError.product("Grok 行缺少 \(labels)：\(texts.map(\.value))")
        }
        guard !texts.contains(where: { ["Not supported", "Unavailable", "不支持", "不可用"].contains($0.value) }) else {
            throw E2EError.product("Grok 不得显示不支持或不可用")
        }
    }

    private static func waitRow(_ session: Session, anyOf labels: [String]) async throws -> Frame {
        let deadline = Date().addingTimeInterval(12)
        while Date() < deadline {
            let frame = try await row(session)
            if (try? require(frame, anyOf: labels)) != nil { return frame }
            try await Task.sleep(nanoseconds: 150_000_000)
        }
        throw E2EError.product("Grok 行未进入 \(labels)")
    }

    private static func action(_ session: Session, labels: [String], hover: Bool = false) async throws {
        let frame = try await row(session)
        let texts = try rowText(frame)
        let point: CGPoint
        if let button = texts.filter({ text in labels.contains { text.value == $0 } })
            .max(by: { $0.center.x < $1.center.x }) {
            point = button.center
        } else if let status = texts.first(where: { ["未安装", "就绪"].contains($0.value) }) {
            // 深色中文按钮的两字标签会被 OCR 漏读。以本行状态的中心 y 定位同高按钮；
            // 看图核对按钮中心距设置面板右缘 84pt，面板宽度遵循现有 SettingsSupport 的计算。
            let width = frame.windowFrame.width
            let panel = min(max(width * 0.7, 760), min(1280, width - 64))
            point = CGPoint(x: (width + panel) / 2 - 84, y: status.center.y)
        } else {
            throw E2EError.product("Grok 行找不到按钮及状态锚点 \(labels)：\(texts.map(\.value))")
        }
        if hover { try await session.hover(local: point, on: frame) }
        else { try await session.click(local: point, on: frame) }
    }
}
