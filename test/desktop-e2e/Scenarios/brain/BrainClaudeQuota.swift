import AppKit
import E2EKit
import Fixtures
import Foundation

/// 当前工作树真实 App + 无网络 ACP 回放；不代表历史 ACP 流或实际语音播放。
@MainActor
public enum BrainClaudeQuota {
    public nonisolated static let name = "brain-claude-quota"

    public static func run(_ session: Session) async throws {
        var project: URL?, shell: URL?, brain: URL?
        var firstTurn = 0, finalTurn = 0
        defer {
            if let brain, let output = session.output {
                for name in ["adapter-output.json", "replay.jsonl", "config.json"] {
                    try? FileManager.default.copyItem(at: brain.appendingPathComponent(name), to: output.appendingPathComponent(name))
                }
            }
            session.stop()
            for path in [project, shell, brain].compactMap({ $0 }) { try? FileManager.default.removeItem(at: path) }
        }
        try await session.step("安装无网络 Claude consumer 回放并触发真实 Agent 完成事件") {
            shell = try ClaudeQuotaFixture.installAgents()
            brain = try ClaudeQuotaFixture.install()
            setenv("ZDOTDIR", shell!.path, 1)
            setenv("NORMA_BRAIN_DIR", brain!.path, 1)
            setenv("NORMA_E2E_LANGUAGE", "zh-Hans", 1)
            try session.launch()
            try await BrainSupport.createWorkspace(session, name: "ClaudeQuota")
            session.stop()
            guard let data = session.dataRoot else { throw E2EError.env("隔离目录丢失") }
            project = try CanvasProjectFixture.install(dataRoot: data, workspaceName: "ClaudeQuota")
            try session.launch()
            try await BrainSupport.openProjectWithAgents(session, count: 1)
            try await waitFile(brain!.appendingPathComponent("completed-1"))
            let rows = try await waitTurns(session, reasons: ["error"])
            firstTurn = rows[0]["turn"] as? Int ?? 0
            try saveJSON(rows, session, "quota-first-turn.json")
            return "首次额度错误已真实落库，stop_reason=error"
        }
        try await session.step("展开额度失败轮次，正文与错误原因可读") {
            try await session.shortcut(key: 11, modifiers: [0x37, 0x38])
            try await showQuota(session, turn: firstTurn, sessionID: String(contentsOf: brain!.appendingPathComponent("completed-1"), encoding: .utf8), screenshot: "quota-first-panel.png")
            try await session.shortcut(key: 11, modifiers: [0x37, 0x38])
            return "面板能读到 session limit 和 rate_limit；失败不是进行中"
        }
        try await session.step("在额度正文已收到、终止响应未发出时，通过设置重启幕僚") {
            try await session.shortcut(key: 43, modifiers: [0x37])
            _ = try await SettingsSupport.select(session, labels: ["幕僚"], content: ["修改后自动重启", "汇报"])
            // 先定位可点的策略，再触发第二轮，避免等待界面造成不确定时序。
            try await BrainSupport.clickScrolling(session, "速度优先")
            try Data().write(to: brain!.appendingPathComponent("trigger-2"))
            try await waitFile(brain!.appendingPathComponent("pending-ready"))
            try await BrainSupport.clickScrolling(session, "质量优先")
            let deadline = Date().addingTimeInterval(8)
            var changed = false
            while Date() < deadline {
                let config = try String(contentsOf: brain!.appendingPathComponent("config.json"), encoding: .utf8)
                if config.contains("\"quality\"") { changed = true; break }
                try await Task.sleep(nanoseconds: 100_000_000)
            }
            guard changed else { throw E2EError.product("设置未写入 quality") }
            try session.save(try await session.capture(), "quota-settings-restart.png")
            let rows = try await waitTurns(session, reasons: ["error", "cancelled"])
            try saveJSON(rows, session, "quota-restart-turns.json")
            try await session.shortcut(key: 53, modifiers: [])
            return "旧轮正文保留且明确 cancelled；未发出的错误响应不伪造成已接收"
        }
        try await session.step("新服务再次返回额度错误，核对唯一轮次与完整持久化") {
            try Data().write(to: brain!.appendingPathComponent("trigger-3"))
            try await waitFile(brain!.appendingPathComponent("completed-3"))
            let rows = try await waitTurns(session, reasons: ["error", "cancelled", "error"])
            let database = try session.databaseURL()
            let detailDeadline = Date().addingTimeInterval(10)
            var items: [[String: Any]] = []
            repeat {
                items = try ClaudeQuotaFixture.rows(database, sql: "SELECT * FROM brain_trace_items ORDER BY at_ms,rowid")
                if items.filter({ $0["kind"] as? String == "error" && ($0["text"] as? String)?.contains("rate_limit") == true }).count == 2 { break }
                try await Task.sleep(nanoseconds: 100_000_000)
            } while Date() < detailDeadline
            let ids = rows.compactMap { $0["turn"] as? Int }
            guard Set(ids).count == 3 else { throw E2EError.product("轮次 ID 重复：\(rows)") }
            for (index, id) in ids.enumerated() {
                let row = rows[index]
                let same = items.filter {
                    ($0["turn"] as? Int) == id && ($0["launch_id"] as? Int) == (row["launch_id"] as? Int)
                    && ($0["area_id"] as? String) == (row["area_id"] as? String) && $0["lane"] as? String == "Main"
                }
                let marker = ["completed-1", "pending-ready", "completed-3"][index]
                let expectedSession = try String(contentsOf: brain!.appendingPathComponent(marker), encoding: .utf8)
                guard same.contains(where: { $0["kind"] as? String == "lifecycle" && ($0["text"] as? String)?.contains("session_id=" + expectedSession) == true }) else {
                    throw E2EError.product("持久化轮次与原始 ACP 会话不匹配")
                }
                guard same.filter({ $0["kind"] as? String == "turn" }).count == 1,
                      same.contains(where: { $0["kind"] as? String == "reply" && ($0["text"] as? String)?.contains("session limit") == true }),
                      same.filter({ $0["kind"] as? String == "error" && ($0["text"] as? String)?.contains("rate_limit") == true }).count == (index == 1 ? 0 : 1)
                else { throw E2EError.product("正文或终止状态串轮／丢失：\(same)") }
            }
            finalTurn = ids[2]
            try saveJSON(rows, session, "quota-final-turns.json")
            try saveJSON(items, session, "quota-final-items.json")
            try await session.shortcut(key: 11, modifiers: [0x37, 0x38])
            try await showQuota(session, turn: finalTurn, sessionID: String(contentsOf: brain!.appendingPathComponent("completed-3"), encoding: .utf8), screenshot: "quota-final-panel.png")
            return "三轮分别 error / cancelled / error，各有唯一结束；新旧会话分开"
        }
        try await session.step("重开隔离工作树 App，从 SQLite 还原额度错误面板") {
            guard let output = session.output else { throw E2EError.env("缺少证据目录") }
            try FileManager.default.copyItem(at: output.appendingPathComponent("app.log"), to: output.appendingPathComponent("app-before-reopen.log"))
            session.stop()
            try session.launch()
            try await BrainSupport.openProjectWithAgents(session, count: 0)
            _ = try await waitTurns(session, reasons: ["error", "cancelled", "error"])
            try await session.shortcut(key: 11, modifiers: [0x37, 0x38])
            try await showQuota(session, turn: finalTurn, sessionID: String(contentsOf: brain!.appendingPathComponent("completed-3"), encoding: .utf8), screenshot: "quota-reopened-panel.png")
            return "重新启动后旧轮错误仍可读；没有注入任何活动数据库行"
        }
    }

    private static func showQuota(_ session: Session, turn expected: Int, sessionID: String, screenshot: String) async throws {
        _ = try await session.wait(Expectation(anyOf: ["幕僚活动", "大脑活动"], timeout: 12))
        // The panel keeps its scroll offset while closed; return to the target row first.
        try await session.scroll(xFraction: 0.65, yFraction: 0.6, deltaY: 4000)
        let labels = ["第 \(expected) 轮", "第\(expected)轮", "第\(expected) 轮", "第 \(expected)轮"]
        let panel = try await session.wait(Expectation(anyOf: labels, timeout: 12))
        guard let turn = panel.pick(labels, .top) else { throw E2EError.product("找不到目标第 \(expected) 轮：\(panel.summary)") }
        try await session.click(local: turn.center, on: panel)
        let identity = try await session.wait(Expectation(allOf: ["会话关联"], timeout: 10))
        try session.save(identity, screenshot.replacingOccurrences(of: ".png", with: "-identity.png"))
        var frame = try await session.capture()
        for _ in 0..<14 {
            if frame.contains("复制本轮") { break }
            try await session.scroll(xFraction: 0.65, yFraction: 0.6, deltaY: -200)
            frame = try await session.capture()
        }
        guard frame.contains("复制本轮"), frame.contains("本轮失败"), frame.contains("本轮结束") else {
            throw E2EError.product("额度错误与结束条目未显示：\(frame.summary)")
        }
        try session.save(frame, screenshot)
        // Vision can omit whole English lines among Chinese text. Use the real UI's copy
        // action for exact identity/content assertions, plus saved screenshots for legibility.
        let clipboard = NSPasteboard.general
        let saved = (clipboard.pasteboardItems ?? []).map { item in
            let copy = NSPasteboardItem()
            for type in item.types { if let data = item.data(forType: type) { copy.setData(data, forType: type) } }
            return copy
        }
        defer { clipboard.clearContents(); if !saved.isEmpty { clipboard.writeObjects(saved) } }
        clipboard.clearContents()
        try await session.click(matching: ["复制本轮"], pick: .top)
        let deadline = Date().addingTimeInterval(4)
        var copied = ""
        repeat {
            copied = clipboard.string(forType: .string) ?? ""
            if !copied.isEmpty { break }
            try await Task.sleep(nanoseconds: 100_000_000)
        } while Date() < deadline
        guard copied.contains("session_id=" + sessionID), copied.contains("rate_limit"),
              copied.contains("You've hit your session limit · resets 6:30am (America/Los_Angeles)"),
              copied.contains("stopReason=error") else {
            throw E2EError.product("真实面板复制正文与目标会话／额度原因不符")
        }
        if let output = session.output {
            try copied.write(to: output.appendingPathComponent(screenshot.replacingOccurrences(of: ".png", with: ".txt")), atomically: true, encoding: .utf8)
        }
    }

    private static func waitTurns(_ session: Session, reasons: [String]) async throws -> [[String: Any]] {
        let deadline = Date().addingTimeInterval(15)
        var rows: [[String: Any]] = []
        while Date() < deadline {
            rows = (try? ClaudeQuotaFixture.rows(session.databaseURL(), sql: "SELECT * FROM brain_turns ORDER BY started_at_ms,turn")) ?? []
            if rows.compactMap({ $0["stop_reason"] as? String }) == reasons,
               rows.allSatisfy({ $0["finished_at_ms"] is NSNumber }) { return rows }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("轮次停止原因预期 \(reasons)，实际 \(rows)")
    }

    private static func waitFile(_ file: URL) async throws {
        let deadline = Date().addingTimeInterval(35)
        while Date() < deadline {
            if FileManager.default.fileExists(atPath: file.path) { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("未收到回放阶段 \(file.lastPathComponent)")
    }

    private static func saveJSON(_ rows: [[String: Any]], _ session: Session, _ name: String) throws {
        guard let output = session.output else { throw E2EError.env("缺少证据目录") }
        try JSONSerialization.data(withJSONObject: rows, options: [.prettyPrinted, .sortedKeys]).write(to: output.appendingPathComponent(name))
    }
}
