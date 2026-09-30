import Foundation
import SQLite3

public struct WorkspaceProjectReference: Codable, Equatable, Sendable, Identifiable {
    public let machineID: String
    public let projectID: String
    public let name: String
    public let machineName: String
    public var id: String { machineID + ":" + projectID }
    public init(machineID: String, projectID: String, name: String, machineName: String) {
        self.machineID = machineID; self.projectID = projectID; self.name = name; self.machineName = machineName
    }
}

public struct WorkspaceAgentReference: Codable, Equatable, Sendable, Identifiable {
    public let machineID: String
    public let projectID: String
    public let definitionID: Int64
    public let name: String
    public let profile: String
    public var projectReferenceID: String { machineID + ":" + projectID }
    public var id: String { projectReferenceID + ":" + String(definitionID) }
    public init(machineID: String, projectID: String, definitionID: Int64, name: String, profile: String) {
        self.machineID = machineID; self.projectID = projectID; self.definitionID = definitionID
        self.name = name; self.profile = profile
    }
}

public struct CollaborationWorkspace: Codable, Equatable, Sendable, Identifiable {
    public let id: String
    public var name: String
    public var projects: [WorkspaceProjectReference]
    public var agents: [WorkspaceAgentReference]
    public var archived: Bool
    public let createdAt: Date
    public var updatedAt: Date
}

public enum CollaborationWorkspaceError: LocalizedError {
    case invalidConfiguration, notFound, damagedStore
    public var errorDescription: String? {
        switch self {
        case .invalidConfiguration: "工作空间配置无效，请检查名称和项目、Agent 关联。"
        case .notFound: "找不到这个工作空间，原有数据未被修改。"
        case .damagedStore: "无法读取工作空间，原有数据已保留。"
        }
    }
}

/// A separate local collaboration identity; projectID remains the Mac protocol's legacy workspace_id.
public actor CollaborationWorkspaceStore {
    private let databaseURL: URL
    // Scheduling fairness only; operation identity and delivery truth remain in SQLite.
    var lastDeliveryAttempt: [String: String] = [:]
    var lastTaskAttempt: [String: String] = [:]
    let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
    public init(databaseURL: URL) { self.databaseURL = databaseURL }

    public func all() throws -> [CollaborationWorkspace] {
        try withDatabase { db in
            let statement = try prepare(db, "SELECT id, configuration FROM collaboration_workspaces ORDER BY updated_at DESC, id")
            defer { sqlite3_finalize(statement) }
            var records: [CollaborationWorkspace] = []
            while true {
                let result = sqlite3_step(statement)
                if result == SQLITE_DONE { return records }
                guard result == SQLITE_ROW, let bytes = sqlite3_column_blob(statement, 1),
                      let id = sqlite3_column_text(statement, 0) else { throw CollaborationWorkspaceError.damagedStore }
                let data = Data(bytes: bytes, count: Int(sqlite3_column_bytes(statement, 1)))
                guard let record = try? JSONDecoder().decode(CollaborationWorkspace.self, from: data),
                      record.id == String(cString: id), (try? validate(record)) != nil else {
                    throw CollaborationWorkspaceError.damagedStore
                }
                records.append(record)
            }
        }
    }

    public func create(name: String, at date: Date = .now) throws -> CollaborationWorkspace {
        let record = CollaborationWorkspace(id: UUID().uuidString, name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                                            projects: [], agents: [], archived: false, createdAt: date, updatedAt: date)
        try validate(record)
        try write(record, inserting: true)
        return record
    }

    public func save(_ record: CollaborationWorkspace, at date: Date = .now) throws {
        guard let existing = try all().first(where: { $0.id == record.id }) else { throw CollaborationWorkspaceError.notFound }
        guard existing.createdAt == record.createdAt else { throw CollaborationWorkspaceError.invalidConfiguration }
        var updated = record
        updated.name = updated.name.trimmingCharacters(in: .whitespacesAndNewlines)
        updated.updatedAt = date
        try validate(updated)
        try write(updated, inserting: false)
    }

    func validate(_ record: CollaborationWorkspace) throws {
        let text: (String) -> Bool = { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && $0.count <= 256 }
        let projects = Set(record.projects.map(\.id))
        guard UUID(uuidString: record.id) != nil, text(record.name),
              record.createdAt.timeIntervalSince1970.isFinite, record.updatedAt.timeIntervalSince1970.isFinite,
              projects.count == record.projects.count, Set(record.agents.map(\.id)).count == record.agents.count,
              record.projects.allSatisfy({ validID($0.machineID) && validID($0.projectID) && text($0.name) && text($0.machineName) }),
              record.agents.allSatisfy({ projects.contains($0.projectReferenceID) && $0.definitionID > 0 && text($0.name) && text($0.profile) }) else {
            throw CollaborationWorkspaceError.invalidConfiguration
        }
    }

    private func write(_ record: CollaborationWorkspace, inserting: Bool) throws {
        let data = try JSONEncoder().encode(record)
        try withDatabase { db in
            let sql = inserting
                ? "INSERT INTO collaboration_workspaces(configuration, updated_at, id) VALUES(?, ?, ?)"
                : "UPDATE collaboration_workspaces SET configuration = ?, updated_at = ? WHERE id = ?"
            let statement = try prepare(db, sql)
            defer { sqlite3_finalize(statement) }
            let bound = data.withUnsafeBytes { sqlite3_bind_blob(statement, 1, $0.baseAddress, Int32($0.count), transient) }
            guard bound == SQLITE_OK,
                  sqlite3_bind_double(statement, 2, record.updatedAt.timeIntervalSince1970) == SQLITE_OK,
                  sqlite3_bind_text(statement, 3, record.id, -1, transient) == SQLITE_OK,
                  sqlite3_step(statement) == SQLITE_DONE, sqlite3_changes(db) == 1 else {
                throw CollaborationWorkspaceError.damagedStore
            }
        }
    }

    func withDatabase<T>(_ operation: (OpaquePointer) throws -> T) throws -> T {
        let directory = databaseURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        #if os(iOS)
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: directory.path)
        #endif
        var connection: OpaquePointer?
        guard sqlite3_open(databaseURL.path, &connection) == SQLITE_OK, let db = connection else {
            if let connection { sqlite3_close(connection) }
            throw CollaborationWorkspaceError.damagedStore
        }
        defer { sqlite3_close(db) }
        guard sqlite3_exec(db, "CREATE TABLE IF NOT EXISTS collaboration_workspaces(id TEXT PRIMARY KEY, configuration BLOB NOT NULL, updated_at REAL NOT NULL)", nil, nil, nil) == SQLITE_OK else {
            throw CollaborationWorkspaceError.damagedStore
        }
        #if os(iOS)
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: databaseURL.path)
        #endif
        return try operation(db)
    }

    func prepare(_ db: OpaquePointer, _ sql: String) throws -> OpaquePointer {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
            throw CollaborationWorkspaceError.damagedStore
        }
        return statement
    }
}
