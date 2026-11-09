import E2EKit
import Fixtures
import Foundation

/// 四家脚本 Agent 共用的启动、作答和 stdout 断言。画面步骤留在各场景里。
@MainActor
enum IslandHookSupport {
    static func boot(_ session: Session, language: String, shell: URL, workspace: String, project: inout URL?) async throws {
        try session.launch(language: language)
        _ = try await session.capturePixels()
        session.stop()
        guard let root = session.dataRoot else { throw E2EError.env("缺少隔离目录") }
        let db = try session.databaseURL()
        try Catalog.seedWorkspace(database: db, name: workspace)
        try Catalog.suppressFeatureGuide(database: db)
        project = try CanvasProjectFixture.install(dataRoot: root, workspaceName: workspace)
        setenv("ZDOTDIR", shell.path, 1)
        try session.launch(language: language)
        try await session.visualClick("点击\(workspace)工作空间卡片")
        try await session.visualClick("点击E2E Project项目卡片进入画布")
    }

    static func relaunch(_ session: Session, language: String, workspace: String) async throws {
        session.stop()
        try session.launch(language: language)
        try await session.visualClick("点击\(workspace)工作空间卡片")
        try await session.visualClick("点击E2E Project项目卡片进入画布")
    }

    static func spawn(_ session: Session, command: String, provider: String, root: URL) async throws {
        try? FileManager.default.removeItem(at: root.appendingPathComponent("\(provider).ready"))
        if let main = try await session.nativeWindows().first(where: { $0.width > 900 }) {
            try await session.selectNativeWindow(main.number)
        }
        try await session.visualClick("点击画布底部命令输入框")
        try await session.type(command)
        try await session.shortcut(key: 36, modifiers: [])
        _ = try await session.visualCheck([
            VisualCriterion(id: "ready", text: "\(provider)卡片终端显示\(provider) question fixture ready，没有 command not found 或启动错误")
        ])
        try await waitFile(root.appendingPathComponent("\(provider).ready"))
    }

    static func island(_ session: Session) async throws -> Int {
        guard let island = try await session.nativeWindows().first(where: { $0.width >= 300 && $0.width < 700 }) else {
            throw E2EError.product("缺少独立岛窗口")
        }
        setenv("NORMA_E2E_NO_ACTIVATE", "1", 1)
        try await session.selectNativeWindow(island.number)
        return island.number
    }

    static func trigger(_ root: URL, provider: String, cycle: Int, mode: String) throws {
        try? FileManager.default.removeItem(at: root.appendingPathComponent("\(provider)-\(cycle).json"))
        try Data(mode.utf8).write(to: root.appendingPathComponent("\(provider)-\(cycle)"))
    }

    static func close(_ root: URL, provider: String, cycle: Int) throws {
        try Data().write(to: root.appendingPathComponent("\(provider)-\(cycle)-close"))
    }

    static func waitMeta(_ root: URL, provider: String, cycle: Int, seconds: Double = 40) async throws -> [String: Any] {
        let path = root.appendingPathComponent("\(provider)-\(cycle).json")
        let deadline = Date().addingTimeInterval(seconds)
        while Date() < deadline {
            if let data = try? Data(contentsOf: path),
               let meta = try JSONSerialization.jsonObject(with: data) as? [String: Any] {
                return meta
            }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        let log = (try? String(contentsOf: root.appendingPathComponent("hook.log"), encoding: .utf8)) ?? ""
        throw E2EError.product("\(provider) 第\(cycle)题没有回执。hook.log:\n\(log.suffix(1200))")
    }

    static func stdoutJSON(_ meta: [String: Any]) throws -> [String: Any] {
        let raw = meta["stdout"] as? String ?? ""
        guard let data = raw.data(using: .utf8),
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw E2EError.product("hook stdout 不是 JSON 对象：\(raw) stderr=\(meta["stderr"] ?? "")")
        }
        return object
    }

    static func assertEmptyStdout(_ meta: [String: Any], what: String) throws {
        let raw = (meta["stdout"] as? String ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        let code = meta["returncode"] as? Int ?? -1
        guard raw.isEmpty, code != 124 else {
            throw E2EError.product("\(what) 应马上得到空 stdout，实际 code=\(code) stdout=\(raw) stderr=\(meta["stderr"] ?? "")")
        }
    }

    static func assertGrokDeny(_ meta: [String: Any], reason: String) throws {
        let object = try stdoutJSON(meta)
        guard object["decision"] as? String == "deny", object["reason"] as? String == reason else {
            throw E2EError.product("Grok stdout 不是预期 deny：\(object)")
        }
        guard object["hookSpecificOutput"] == nil, object["updatedInput"] == nil, object["answers"] == nil else {
            throw E2EError.product("Grok stdout 残留了 Claude/桥接字段：\(object)")
        }
    }

    static func assertKimiDeny(_ meta: [String: Any], reason: String) throws {
        let object = try stdoutJSON(meta)
        guard object["decision"] == nil else { throw E2EError.product("Kimi 不应有顶层 decision：\(object)") }
        let specific = object["hookSpecificOutput"] as? [String: Any]
        guard specific?["permissionDecision"] as? String == "deny",
              specific?["permissionDecisionReason"] as? String == reason,
              specific?["updatedInput"] == nil else {
            throw E2EError.product("Kimi stdout 不是 permissionDecisionReason：\(object)")
        }
    }

    static func assertZCodeAllow(_ meta: [String: Any], prompt: String) throws {
        let object = try stdoutJSON(meta)
        guard object["decision"] == nil else { throw E2EError.product("ZCode 不应走顶层 deny：\(object)") }
        let specific = object["hookSpecificOutput"] as? [String: Any]
        let updated = specific?["updatedInput"] as? [String: Any]
        let answers = updated?["answers"] as? [String: String]
        let questions = updated?["questions"] as? [Any]
        guard specific?["permissionDecision"] as? String == "allow",
              answers?[prompt] == "Blue",
              questions?.count == 1 else {
            throw E2EError.product("ZCode stdout 没有完整 updatedInput：\(object)")
        }
    }

    static func assertInfCodeDeny(_ meta: [String: Any], reason: String) throws {
        let object = try stdoutJSON(meta)
        let specific = object["hookSpecificOutput"] as? [String: Any]
        guard object["decision"] as? String == "deny",
              object["reason"] as? String == reason,
              specific?["permissionDecisionReason"] as? String == reason,
              object["answers"] == nil else {
            throw E2EError.product("InfCode stdout 不是双通道 deny：\(object)")
        }
    }

    static func waitForAlert(_ session: Session, number: Int) async throws {
        for _ in 0..<150 {
            if try await session.nativeWindows().contains(where: { $0.number == number && $0.height > 60 }) { return }
            try await Task.sleep(nanoseconds: 200_000_000)
        }
        throw E2EError.product("提问未使岛弹出")
    }

    static func waitForCompact(_ session: Session, number: Int) async throws {
        for _ in 0..<80 {
            if try await session.nativeWindows().contains(where: {
                $0.number == number && $0.width < 400 && $0.height <= 60
            }) { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("岛卡未收回为 Compact")
    }

    static func nativeQuestions(_ session: Session, expected: Bool) async throws {
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            if let raw = try session.setting("island.preferences.v1"),
               let data = raw.data(using: .utf8),
               let saved = try JSONSerialization.jsonObject(with: data) as? [String: Any],
               saved["native_questions"] as? Bool == expected {
                return
            }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        throw E2EError.product("native_questions 未变成 \(expected)")
    }

    private static func waitFile(_ url: URL) async throws {
        for _ in 0..<100 {
            if FileManager.default.fileExists(atPath: url.path) { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw E2EError.product("未看到 \(url.lastPathComponent)")
    }
}
