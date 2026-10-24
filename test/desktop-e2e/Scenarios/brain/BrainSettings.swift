import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 设置 → Norma 大脑：页面内容、开关写入配置、策略选择写入配置。
@MainActor
public enum BrainSettings {
    public nonisolated static let name = "brain-settings"

    public static func run(_ session: Session) async throws {
        var brainDir: URL?
        defer {
            session.stop()
            if let brainDir { try? FileManager.default.removeItem(at: brainDir) }
        }
        let root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-brain-settings-e2e-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        brainDir = root
        let config = root.appendingPathComponent("config.json")
        // 放一个空的适配器可执行文件，让设置页把 Claude Code 判为“已安装”。
        let bin = root.appendingPathComponent("adapters/node_modules/.bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        let adapter = bin.appendingPathComponent("claude-agent-acp")
        try Data("#!/bin/sh\n".utf8).write(to: adapter)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: adapter.path)

        try await session.step("打开设置，进入 Norma 大脑") {
            setenv("NORMA_BRAIN_DIR", root.path, 1)
            setenv("NORMA_E2E_LANGUAGE", "en", 1)
            try await SettingsSupport.open(session)
            let frame = try await SettingsSupport.select(
                session, labels: ["Norma Brain"], content: ["Enable background brain"])
            try session.save(frame, "01-brain-page.png")
            for needle in ["Backend", "Strategy", "Speed first", "Quality first", "Main session model", "Claude Code"] {
                guard frame.contains(needle) else {
                    throw E2EError.product("页面缺少「\(needle)」：\(frame.summary)")
                }
            }
            return "页面上半部分：开关、后端、策略、主会话模型"
        }

        try await session.step("点开关，配置写入 enabled=true") {
            let frame = try await session.capture()
            guard let title = frame.pick(["Enable background brain"], .bottom) else {
                throw E2EError.product("找不到开关标题：\(frame.summary)")
            }
            // 开关在内容卡片右端，垂直居中于卡片，比标题行低约窗口高度的 2.2%。
            try await session.click(
                local: CGPoint(
                    x: Double(frame.windowFrame.width) * 0.805,
                    y: title.center.y + Double(frame.windowFrame.height) * 0.022),
                on: frame)
            try await waitConfig(config, contains: "\"enabled\": true")
            try session.save(try await session.capture(), "02-brain-enabled.png")
            return (try? String(contentsOf: config, encoding: .utf8)) ?? ""
        }

        try await session.step("选择“速度优先”，配置写入 strategy=speed") {
            try await session.click(matching: ["Speed first"], pick: .bottom)
            try await waitConfig(config, contains: "\"strategy\": \"speed\"")
            return (try? String(contentsOf: config, encoding: .utf8)) ?? ""
        }

        try await session.step("向下滚动，核对其余选项都在") {
            var frame = try await session.capture()
            var seen = Set<String>()
            let needles = ["Approval session model", "Periodic summary", "Norma may decide",
                           "Always ask you first", "When judging is slow", "Several approvals at once", "Approval scope"]
            for _ in 0..<8 {
                for needle in needles where frame.contains(needle) { seen.insert(needle) }
                if seen.count == needles.count { break }
                try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: -260)
                frame = try await session.capture()
            }
            try session.save(frame, "03-brain-bottom.png")
            let missing = needles.filter { !seen.contains($0) }
            guard missing.isEmpty else { throw E2EError.product("向下滚动后仍缺少：\(missing)") }
            return "其余选项都在"
        }

        try await session.step("取消勾选“批准在项目目录内写文件”，配置写入 project_writes=false") {
            try await BrainSupport.clickScrolling(session, "Approve file writes inside the project")
            try await waitConfig(config, contains: "\"project_writes\": false")
            return (try? String(contentsOf: config, encoding: .utf8)) ?? ""
        }

        try await session.step("选择“每 10 分钟”与“排队逐个处理”，配置随之改变") {
            try await BrainSupport.clickScrolling(session, "Every 10 min")
            try await waitConfig(config, contains: "\"summary\": \"10m\"")
            try await BrainSupport.clickScrolling(session, "One at a time")
            try await waitConfig(config, contains: "\"parallel_approvals\": false")
            try session.save(try await session.capture(), "04-brain-changed.png")
            return (try? String(contentsOf: config, encoding: .utf8)) ?? ""
        }
    }

    private static func waitConfig(_ url: URL, contains needle: String) async throws {
        let deadline = Date().addingTimeInterval(6)
        while Date() < deadline {
            if let text = try? String(contentsOf: url, encoding: .utf8), text.contains(needle) { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        let now = (try? String(contentsOf: url, encoding: .utf8)) ?? "(没有文件)"
        throw E2EError.product("配置里 6 秒内没有出现 \(needle)，当前：\(now)")
    }
}
