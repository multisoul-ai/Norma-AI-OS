import CryptoKit
import E2EKit
import Foundation

public struct IslandMigrationFixture {
    public let base: URL
    public let scopes: [String]
    public let backups: [URL: Data]
    public var current: URL { base.appendingPathComponent("global/current") }

    public static func seed(database: URL, dataRoot: URL, base: URL) throws -> Self {
        var scopes: [String] = []
        var backups: [URL: Data] = [:]
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        for name in ["Migration Alpha", "Migration Beta"] {
            let root = dataRoot.appendingPathComponent(name)
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            try Catalog.seedWorkspace(database: database, name: name)
            try Catalog.seedArea(database: database, workspaceName: name, root: root, displayName: name)
            guard let scope = try SQLRaw.text(database: database,
                "SELECT 'desktop:' || w.id || ':' || a.id FROM desktop_workspaces w JOIN desktop_project_areas a ON a.desktop_workspace_id=w.id WHERE w.name='\(name)'") else {
                throw E2EError.product("缺少旧项目身份 \(name)")
            }
            scopes.append(scope)
            let old = base.appendingPathComponent("projects/\(hash(scope))")
            try FileManager.default.createDirectory(at: old, withIntermediateDirectories: true)
            try saveJSON([["agent": "Alan", "text": "\(name) original goal"]], to: old.appendingPathComponent("goals.json"))
            try saveJSON(["next_id": 1, "items": [["id": 1, "agent": "Alan", "title": "检查迁移", "task": "\(name) full original task", "why": "保留旧数据", "at": 100]]], to: old.appendingPathComponent("proposals.json"))
            try saveJSON(["always_ask": ["Bash:cargo"]], to: old.appendingPathComponent("config.json"))
            try Data("{\"reply\":\"\(name) original reply\"}\n".utf8).write(to: old.appendingPathComponent("decisions.jsonl"))
            try Data("{\"approval\":\"same-id\",\"decision\":\"Deny\",\"reason\":\"\(name) original audit\"}\n".utf8).write(to: old.appendingPathComponent("approvals.jsonl"))
            for file in ["goals.json", "proposals.json", "config.json", "decisions.jsonl", "approvals.jsonl"] {
                let path = old.appendingPathComponent(file)
                backups[path] = try Data(contentsOf: path)
            }
        }
        let config = base.appendingPathComponent("config.json")
        try saveJSON(["enabled": false, "backend": "claude", "speak": false, "always_ask": ["Write"]], to: config)
        backups[config] = try Data(contentsOf: config)
        try Catalog.suppressFeatureGuide(database: database)
        return Self(base: base, scopes: scopes, backups: backups)
    }

    public func verify(speaking: Bool) throws {
        let goals = try JSONSerialization.jsonObject(with: Data(contentsOf: current.appendingPathComponent("goals.json"))) as? [[String: Any]] ?? []
        guard goals.count == 2, Set(goals.compactMap { $0["workspace_id"] as? String }) == Set(scopes) else {
            throw E2EError.product("迁移后两个目标未完整保留来源")
        }
        let decisions = try String(contentsOf: current.appendingPathComponent("decisions.jsonl"), encoding: .utf8).split(separator: "\n").map {
            try JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] ?? [:]
        }
        guard decisions.count == 4 else { throw E2EError.product("模型决策和审批日志未全部保留") }
        for (index, scope) in scopes.enumerated() {
            let name = ["Migration Alpha", "Migration Beta"][index]
            guard goals.contains(where: { $0["workspace_id"] as? String == scope && $0["text"] as? String == "\(name) original goal" }),
                  decisions.contains(where: { $0["workspace_id"] as? String == scope && $0["reply"] as? String == "\(name) original reply" }),
                  decisions.contains(where: { $0["workspace_id"] as? String == scope && $0["reason"] as? String == "\(name) original audit" }) else {
                throw E2EError.product("原目标或决策日志的正文/来源改变")
            }
            let value = try object(current.appendingPathComponent("proposals/\(Self.hash(scope)).json"))
            let items = value["items"] as? [[String: Any]] ?? []
            guard value["workspace_id"] as? String == scope, items.count == 1,
                  items[0]["workspace_id"] as? String == scope,
                  items[0]["task"] as? String == "\(name) full original task" else {
                throw E2EError.product("旧提议内容或来源缺失")
            }
        }
        let config = try object(current.appendingPathComponent("config.json"))
        let rules = config["always_ask"] as? [[String: Any]] ?? []
        guard config["speak"] as? Bool == speaking, config["enabled"] as? Bool == false, rules.count == 3,
              Set(rules.filter { $0["key"] as? String == "Bash:cargo" }.compactMap { $0["workspace_id"] as? String }) == Set(scopes),
              rules.contains(where: { $0["key"] as? String == "Write" && $0["workspace_id"] is NSNull }) else {
            throw E2EError.product("全局设置或全局/项目确认偏好丢失")
        }
        for (path, bytes) in backups where try Data(contentsOf: path) != bytes {
            throw E2EError.product("旧备份被改变：\(path.lastPathComponent)")
        }
    }

    public func object(_ path: URL) throws -> [String: Any] {
        guard let value = try JSONSerialization.jsonObject(with: Data(contentsOf: path)) as? [String: Any] else {
            throw E2EError.product("不是JSON对象：\(path.lastPathComponent)")
        }
        return value
    }

    private static func hash(_ text: String) -> String {
        SHA256.hash(data: Data(text.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    private static func saveJSON(_ value: Any, to path: URL) throws {
        try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]).write(to: path)
    }
}
