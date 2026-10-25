import E2EKit
import Fixtures
import Foundation

/// 同一真实 App 场景，before 只改变预期；输入和实际 bundle 转换完全一致。
@MainActor
public enum BrainCodexPermission {
    public nonisolated static let name = "brain-codex-permission"

    public static func run(_ session: Session) async throws {
        var project: URL?, shell: URL?, brain: URL?
        let before = ProcessInfo.processInfo.environment["NORMA_BRAIN_REPLAY_EXPECT"] == "before"
        defer {
            if let brain, let output = session.output {
                for name in ["replay.jsonl", "adapter.log"] {
                    try? FileManager.default.copyItem(at: brain.appendingPathComponent(name), to: output.appendingPathComponent(name))
                }
            }
            session.stop()
            for path in [project, shell, brain].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        try await session.step("准备固定 Codex 权限回放与隔离工作空间") {
            shell = try BrainFixture.installFakeAgents()
            brain = try CodexPermissionFixture.install()
            setenv("ZDOTDIR", shell!.path, 1)
            setenv("NORMA_BRAIN_DIR", brain!.path, 1)
            setenv("NORMA_BRAIN_ADAPTER_LOG", brain!.appendingPathComponent("adapter.log").path, 1)
            setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
            try session.launch()
            try await BrainSupport.createWorkspace(session, name: "CodexReplay")
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("隔离目录丢失") }
            project = try CanvasProjectFixture.install(dataRoot: data, workspaceName: "CodexReplay")
            try session.launch()
            try await BrainSupport.openProjectWithAgents(session, count: 1)
            return "真实 App 经 Agent 完成事件触发同一权限脚本"
        }
        try await session.step("核对合法汇报及两项权限拒绝的真实结果") {
            guard let brain else { throw E2EError.env("回放目录丢失") }
            let path = brain.appendingPathComponent("replay.jsonl")
            let deadline = Date().addingTimeInterval(35)
            var entries: [[String: Any]] = []
            while Date() < deadline {
                let text = (try? String(contentsOf: path, encoding: .utf8)) ?? ""
                entries = text.split(separator: "\n").compactMap {
                    (try? JSONSerialization.jsonObject(with: Data($0.utf8))) as? [String: Any]
                }
                if entries.filter({ $0["allowed"] != nil }).count >= 3 { break }
                try await Task.sleep(nanoseconds: 300_000_000)
            }
            let permissions = entries.filter { $0["allowed"] != nil }
            guard permissions.count == 3 else { throw E2EError.product("没有收到完整的三项权限回放：\(entries)") }
            let expected = [!before, false, false]
            guard permissions.compactMap({ $0["allowed"] as? Bool }) == expected else {
                throw E2EError.product("权限结果不符，预期 \(expected)，实际 \(permissions)")
            }
            let delivered = entries.contains { ($0["reportExit"] as? Int) == 0 }
            guard delivered == !before else { throw E2EError.product("汇报 CLI 送达结果不符") }
            guard let output = session.output else { throw E2EError.env("证据目录丢失") }
            let appLog = try String(contentsOf: output.appendingPathComponent("app.log"), encoding: .utf8)
            let received = appLog.split(separator: "\n").contains {
                $0.contains("大脑播报") && $0.contains("Codex quote replay delivered")
            }
            guard received == !before else {
                throw E2EError.product("App 汇报接收日志与 CLI 结果不一致")
            }
            return before ? "修复前：合法汇报被误拒绝，未送达；危险命令拒绝" : "修复后：合法汇报送达；非允许程序和复合命令仍拒绝"
        }
        try await session.step("真实活动面板可见权限拒绝，保存展开截图") {
            try await Task.sleep(nanoseconds: 1_000_000_000)
            try await session.shortcut(key: 11, modifiers: [0x37, 0x38])
            let panel = try await session.wait(Expectation(anyOf: ["幕僚活动", "大脑活动"], timeout: 12))
            try session.save(panel, "codex-activity.png")
            // 新旧面板轮次均可点击；取包含轮次标识的首行展开。
            if let turn = panel.pick(["第1轮", "第 1 轮", "主会话"], .top) {
                try await session.click(local: turn.center, on: panel)
            }
            _ = try await session.wait(Expectation(anyOf: ["发给大脑", "会话关联"], timeout: 12))
            var expanded = try await session.capture()
            for _ in 0..<8 {
                if expanded.contains("拒绝") { break }
                try await session.scroll(xFraction: 0.65, yFraction: 0.6, deltaY: -220)
                expanded = try await session.capture()
            }
            guard expanded.contains("拒绝") else { throw E2EError.product("展开正文中仍未看到权限拒绝") }
            try session.save(expanded, "codex-permission-expanded.png")
            return expanded.summary
        }
    }
}
