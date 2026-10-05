import Foundation
import SQLite3

/// Only reads the pre-workspace-chat database. It never migrates, creates, sends or rewrites old operations.
public actor LegacyWorkspaceHistoryStore {
    private let databaseURL: URL
    public init(databaseURL: URL) { self.databaseURL = databaseURL }

    public func all() throws -> [CollaborationWorkspace] {
        try read { db in
            try rows(db, table: "collaboration_workspaces", payload: "configuration", orderedBy: "updated_at DESC, id")
                .map { try decode(CollaborationWorkspace.self, $0) }
        }
    }
    public func threads(workspaceID: String) throws -> [WorkThread] {
        try read { db in
            try rows(db, table: "work_threads", scope: "workspace_id", value: workspaceID, orderedBy: "rowid DESC")
                .map { row in
                    let record = try decode(WorkThread.self, row)
                    guard record.workspaceID == workspaceID else { throw WorkThreadError.damagedStore }
                    return record
                }
        }
    }
    public func messages(threadID: String) throws -> [WorkThreadMessage] {
        try read { db in
            try rows(db, table: "work_messages", scope: "thread_id", value: threadID)
                .map { row in
                    let record = try decode(WorkThreadMessage.self, row)
                    guard record.threadID == threadID else { throw WorkThreadError.damagedStore }
                    return record
                }
        }
    }
    public func messageDraft(threadID: String) throws -> WorkThreadDraft? {
        try read { db in
            try rows(db, table: "work_thread_drafts", id: "thread_id", scope: "thread_id", value: threadID)
                .first.map { try JSONDecoder().decode(WorkThreadDraft.self, from: $0.data) }
        }
    }
    public func legacyThreadDeliveries(threadID: String) throws -> [ThreadOutboxOperation] {
        let messageIDs = Set(try messages(threadID: threadID).map(\.id))
        return try read { db in
            try rows(db, table: "thread_outbox").map { try decode(ThreadOutboxOperation.self, $0) }
                .filter { messageIDs.contains($0.messageID) }
        }
    }
    public func legacyThreadTasks(threadID: String) throws -> [ThreadTaskOutboxOperation] {
        try read { db in
            try rows(db, table: "thread_task_outbox", scope: "thread_id", value: threadID)
                .map { row in
                    let record = try decode(ThreadTaskOutboxOperation.self, row)
                    guard record.task.threadID == threadID else { throw WorkThreadError.damagedStore }
                    return record
                }
        }
    }

    private func read<T>(_ operation: (OpaquePointer) throws -> T) throws -> T where T: RangeReplaceableCollection {
        guard FileManager.default.fileExists(atPath: databaseURL.path) else { return T() }
        return try open(operation)
    }
    private func read<T>(_ operation: (OpaquePointer) throws -> T?) throws -> T? {
        guard FileManager.default.fileExists(atPath: databaseURL.path) else { return nil }
        return try open(operation)
    }
    private func open<T>(_ operation: (OpaquePointer) throws -> T) throws -> T {
        var connection: OpaquePointer?
        let result = sqlite3_open_v2(databaseURL.path, &connection, SQLITE_OPEN_READONLY, nil)
        guard result == SQLITE_OK, let db = connection else {
            if let connection { sqlite3_close(connection) }
            throw WorkThreadError.damagedStore
        }
        defer { sqlite3_close(db) }
        return try operation(db)
    }
    private func rows(_ db: OpaquePointer, table: String, id: String = "id", payload: String = "payload",
                      scope: String? = nil, value: String? = nil, orderedBy: String = "rowid") throws -> [(id: String, data: Data)] {
        // Identifiers are private literals; user identifiers are always bound parameters.
        let exists = try statement(db, "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?")
        defer { sqlite3_finalize(exists) }
        try bind(exists, table)
        if sqlite3_step(exists) == SQLITE_DONE { return [] }
        let query = "SELECT \(id),\(payload) FROM \(table)" + (scope.map { " WHERE \($0)=?" } ?? "") + " ORDER BY \(orderedBy)"
        let stmt = try statement(db, query)
        defer { sqlite3_finalize(stmt) }
        if let value { try bind(stmt, value) }
        var records: [(String, Data)] = []
        while true {
            let result = sqlite3_step(stmt)
            if result == SQLITE_DONE { return records }
            guard result == SQLITE_ROW, let key = sqlite3_column_text(stmt, 0), let bytes = sqlite3_column_blob(stmt, 1) else {
                throw WorkThreadError.damagedStore
            }
            records.append((String(cString: key), Data(bytes: bytes, count: Int(sqlite3_column_bytes(stmt, 1)))))
        }
    }
    private func statement(_ db: OpaquePointer, _ sql: String) throws -> OpaquePointer {
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK, let stmt else { throw WorkThreadError.damagedStore }
        return stmt
    }
    private func bind(_ stmt: OpaquePointer, _ text: String) throws {
        let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
        guard sqlite3_bind_text(stmt, 1, text, -1, transient) == SQLITE_OK else { throw WorkThreadError.damagedStore }
    }
    private func decode<T: Decodable & Identifiable>(_ type: T.Type, _ row: (id: String, data: Data)) throws -> T where T.ID == String {
        guard let record = try? JSONDecoder().decode(type, from: row.data), record.id == row.id,
              UUID(uuidString: row.id) != nil else { throw WorkThreadError.damagedStore }
        return record
    }
}
