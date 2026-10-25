import Foundation
import SQLite3

public struct AgentDefinitionRow: Equatable {
    public let id: Int
    public let name: String
    public let alias: String
    public let profile: String
}

public struct DispatchTaskRow: Equatable {
    public let definitionId: Int
    public let taskText: String
}

extension Session {
    /// 把一句转写当作豆包 ASR 的最终结果注入真实会话；之后走生产的直呼/模型路径。
    public func doubaoTranscript(_ text: String) async throws {
        try await control(["op": "doubao_transcript", "text": text])
    }

    /// 桌面工作空间按项目拆库：Agent 定义在 desktop-project-definitions，派发任务在 desktop-canvases。
    func uniqueDatabase(in directoryName: String) throws -> URL {
        guard let dataRoot else { throw E2EError.env("数据目录尚未建立") }
        let directory = dataRoot.appendingPathComponent(directoryName, isDirectory: true)
        let found = ((try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? [])
            .filter { $0.pathExtension == "sqlite" }
        guard found.count == 1, let url = found.first else {
            throw E2EError.product("\(directoryName) 应恰有一个库，实际 \(found.count) 个")
        }
        return url
    }

    public func agentDefinitions() throws -> [AgentDefinitionRow] {
        let database = try SQLiteDB(path: uniqueDatabase(in: "desktop-project-definitions").path, write: false)
        defer { database.close() }
        return try database.query("SELECT id,name,COALESCE(speech_alias,''),runtime_profile FROM agent_definitions ORDER BY id") {
            AgentDefinitionRow(id: Int(sqlite3_column_int64($0, 0)), name: column($0, 1), alias: column($0, 2), profile: column($0, 3))
        }
    }

    public func dispatchTasks() throws -> [DispatchTaskRow] {
        let database = try SQLiteDB(path: uniqueDatabase(in: "desktop-canvases").path, write: false)
        defer { database.close() }
        return try database.query("SELECT definition_id,task_text FROM agent_dispatch_tasks ORDER BY id") {
            DispatchTaskRow(definitionId: Int(sqlite3_column_int64($0, 0)), taskText: column($0, 1))
        }
    }
}
