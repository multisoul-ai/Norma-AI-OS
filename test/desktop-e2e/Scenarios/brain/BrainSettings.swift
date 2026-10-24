import CoreGraphics
import E2EKit
import Fixtures
import Foundation

/// 设置 → 幕僚：页面内容、开关与分段按钮写入配置。
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

        try await session.step("打开设置，进入幕僚") {
            setenv("NORMA_BRAIN_DIR", root.path, 1)
            setenv("NORMA_E2E_LANGUAGE", "en", 1)
            try await SettingsSupport.open(session)
            let frame = try await SettingsSupport.select(
                session, labels: ["Chief of Staff"], content: ["Restarts when changed"])
            try session.save(frame, "01-brain-page.png")
            for needle in ["Stopped", "Reports", "Voice reports", "Periodic summary"] {
                guard frame.contains(needle) else {
                    throw E2EError.product("页面缺少「\(needle)」：\(frame.summary)")
                }
            }
            return "页面上半部分：状态卡、汇报分组"
        }

        try await session.step("点状态卡的开关，配置写入 enabled=true") {
            try await clickRowSwitch(session, "Restarts when changed", lift: -9)
            try await waitConfig(config, contains: "\"enabled\": true")
            try session.save(try await session.capture(), "02-brain-enabled.png")
            return (try? String(contentsOf: config, encoding: .utf8)) ?? ""
        }

        try await session.step("关掉“语音汇报”，配置写入 speak=false") {
            try await clickRowSwitch(session, "Voice reports")
            try await waitConfig(config, contains: "\"speak\": false")
            return (try? String(contentsOf: config, encoding: .utf8)) ?? ""
        }

        try await session.step("向下滚动，核对其余分组与选项都在") {
            var frame = try await session.capture()
            var seen = Set<String>()
            let needles = ["When judging is slow", "Next steps & autonomy", "Follow-up tasks", "Always ask you first",
                           "Models", "Claude Code", "Strategy", "Main session model", "Approval session model",
                           "Advanced", "Judge approvals in parallel", "Approval scope"]
            for _ in 0..<10 {
                for needle in needles where frame.contains(needle) { seen.insert(needle) }
                if seen.count == needles.count { break }
                try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: -260)
                frame = try await session.capture()
            }
            try session.save(frame, "03-brain-bottom.png")
            let missing = needles.filter { !seen.contains($0) }
            guard missing.isEmpty else { throw E2EError.product("向下滚动后仍缺少：\(missing)") }
            return "其余分组都在"
        }

        try await session.step("选择“速度优先”，配置写入 strategy=speed") {
            try await BrainSupport.clickScrolling(session, "Speed first")
            try await waitConfig(config, contains: "\"strategy\": \"speed\"")
            return (try? String(contentsOf: config, encoding: .utf8)) ?? ""
        }

        try await session.step("关掉“放行项目内写文件”和“并行处理审批”，选“10 min”，配置随之改变") {
            try await clickRowSwitch(session, "Approve writes inside the project")
            try await waitConfig(config, contains: "\"project_writes\": false")
            try await clickRowSwitch(session, "Judge approvals in parallel")
            try await waitConfig(config, contains: "\"parallel_approvals\": false")
            try await BrainSupport.clickScrolling(session, "10 min")
            try await waitConfig(config, contains: "\"summary\": \"10m\"")
            try session.save(try await session.capture(), "04-brain-changed.png")
            return (try? String(contentsOf: config, encoding: .utf8)) ?? ""
        }

        try await session.step("切到浅色主题 Paper Blue，回到幕僚页看上下两部分") {
            // “Paper Blue”是浅色主题（纸白 · 钴蓝）。
            _ = try await SettingsSupport.select(session, labels: ["Themes"], content: ["Paper Blue"])
            try await session.click(matching: ["Paper Blue"], pick: .bottom)
            try await Task.sleep(nanoseconds: 800_000_000)
            let top = try await SettingsSupport.select(
                session, labels: ["Chief of Staff"], content: ["Restarts when changed"])
            try session.save(top, "05-brain-light-top.png")
            for _ in 0..<6 { try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: -260) }
            try session.save(try await session.capture(), "06-brain-light-bottom.png")
            return "浅色主题截图已保存"
        }
    }

    /// 开关在行的右端，与行标题和说明两行一起垂直居中：比标题低约 9pt。看不到标题时先滚动。
    private static func clickRowSwitch(_ session: Session, _ label: String, lift: Double = 9) async throws {
        for step in 0..<10 {
            let frame = try await session.capture()
            if let text = frame.pick([label], .bottom) {
                try await session.click(
                    local: CGPoint(x: Double(frame.windowFrame.width) * 0.805, y: text.center.y + lift),
                    on: frame)
                return
            }
            try await session.scroll(xFraction: 0.7, yFraction: 0.65, deltaY: step < 5 ? -260 : 260)
        }
        throw E2EError.product("滚动后仍找不到「\(label)」")
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
