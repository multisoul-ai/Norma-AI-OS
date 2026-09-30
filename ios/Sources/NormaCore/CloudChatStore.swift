import Foundation
import SQLite3

public enum CloudChatRole: String, Sendable { case user, norma }
public enum CloudChatState: String, Sendable { case pending, complete, failed }

public struct CloudChatMessage: Identifiable, Equatable, Sendable {
    public let id: String
    public let turnID: String
    public let role: CloudChatRole
    public let text: String
    public let state: CloudChatState
    public let errorCode: String?
    public let createdAt: Date
}

public enum CloudChatStoreError: LocalizedError {
    case invalidMessage, invalidTurn, damagedStore
    public var errorDescription: String? {
        switch self {
        case .invalidMessage: "请输入文字后再发送。"
        case .invalidTurn: "找不到这轮对话，原始记录未更改。"
        case .damagedStore: "无法读取 Norma 对话记录；原始数据已保留。"
        }
    }
}

/// Long-term, app-private visible messages. Cloud inference never owns this history.
public actor CloudChatStore {
    private let databaseURL: URL
    private let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
    private var protectionApplied = false

    public init(databaseURL: URL) { self.databaseURL = databaseURL }

    public func all() throws -> [CloudChatMessage] {
        try withDatabase { db in
            let statement = try prepare(db, "SELECT id, turn_id, role, text, state, error_code, created_at FROM messages ORDER BY created_at ASC, rowid ASC")
            defer { sqlite3_finalize(statement) }
            var messages: [CloudChatMessage] = []
            while true {
                let result = sqlite3_step(statement)
                if result == SQLITE_DONE { return messages }
                guard result == SQLITE_ROW else { throw CloudChatStoreError.damagedStore }
                messages.append(try read(statement))
            }
        }
    }

    public func search(_ query: String) throws -> [CloudChatMessage] {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !term.isEmpty else { return try all() }
        // ponytail: local visible history is small; add FTS only when this scan is measurably slow.
        return try all().filter { $0.text.localizedStandardContains(term) }
    }

    public func appendUser(turnID: String, text: String, at date: Date = .now) throws -> CloudChatMessage {
        guard UUID(uuidString: turnID) != nil else { throw CloudChatStoreError.invalidTurn }
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              text.unicodeScalars.count <= 4096 else { throw CloudChatStoreError.invalidMessage }
        let message = CloudChatMessage(id: UUID().uuidString, turnID: turnID, role: .user,
                                       text: text, state: .pending, errorCode: nil, createdAt: date)
        try withDatabase { db in try insert(db, message) }
        return message
    }

    public func complete(turnID: String, text: String, at date: Date = .now) throws -> CloudChatMessage {
        guard UUID(uuidString: turnID) != nil else { throw CloudChatStoreError.invalidTurn }
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              text.utf8.count <= 65_536 else { throw CloudChatStoreError.invalidMessage }
        let reply = CloudChatMessage(id: UUID().uuidString, turnID: turnID, role: .norma,
                                     text: text, state: .complete, errorCode: nil, createdAt: date)
        try withDatabase { db in
            guard sqlite3_exec(db, "BEGIN IMMEDIATE", nil, nil, nil) == SQLITE_OK else {
                throw CloudChatStoreError.damagedStore
            }
            do {
                let statement = try prepare(db, "UPDATE messages SET state = 'complete' WHERE turn_id = ? AND role = 'user' AND state = 'pending'")
                defer { sqlite3_finalize(statement) }
                try bind(turnID, to: statement, at: 1)
                guard sqlite3_step(statement) == SQLITE_DONE, sqlite3_changes(db) == 1 else {
                    throw CloudChatStoreError.invalidTurn
                }
                try insert(db, reply)
                guard sqlite3_exec(db, "COMMIT", nil, nil, nil) == SQLITE_OK else {
                    throw CloudChatStoreError.damagedStore
                }
            } catch {
                sqlite3_exec(db, "ROLLBACK", nil, nil, nil)
                throw error
            }
        }
        return reply
    }

    public func fail(turnID: String, code: String) throws {
        guard UUID(uuidString: turnID) != nil, !code.isEmpty, code.utf8.count <= 80 else {
            throw CloudChatStoreError.invalidTurn
        }
        try withDatabase { db in
            let statement = try prepare(db, "UPDATE messages SET state = 'failed', error_code = ? WHERE turn_id = ? AND role = 'user' AND state = 'pending'")
            defer { sqlite3_finalize(statement) }
            try bind(code, to: statement, at: 1)
            try bind(turnID, to: statement, at: 2)
            guard sqlite3_step(statement) == SQLITE_DONE, sqlite3_changes(db) == 1 else {
                throw CloudChatStoreError.invalidTurn
            }
        }
    }

    private func withDatabase<T>(_ operation: (OpaquePointer) throws -> T) throws -> T {
        try FileManager.default.createDirectory(at: databaseURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        var connection: OpaquePointer?
        let flags = SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX
        guard sqlite3_open_v2(databaseURL.path, &connection, flags, nil) == SQLITE_OK, let db = connection else {
            if let connection { sqlite3_close(connection) }
            throw CloudChatStoreError.damagedStore
        }
        defer { sqlite3_close(db) }
        let schema = """
            CREATE TABLE IF NOT EXISTS messages (
              id TEXT PRIMARY KEY NOT NULL, turn_id TEXT NOT NULL, role TEXT NOT NULL,
              text TEXT NOT NULL, state TEXT NOT NULL, error_code TEXT, created_at REAL NOT NULL,
              UNIQUE(turn_id, role));
            """
        guard sqlite3_exec(db, schema, nil, nil, nil) == SQLITE_OK else { throw CloudChatStoreError.damagedStore }
        #if os(iOS)
        if !protectionApplied {
            try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete],
                ofItemAtPath: databaseURL.deletingLastPathComponent().path)
            try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete],
                ofItemAtPath: databaseURL.path)
            protectionApplied = true
        }
        #endif
        return try operation(db)
    }

    private func insert(_ db: OpaquePointer, _ message: CloudChatMessage) throws {
        let statement = try prepare(db, "INSERT INTO messages (id, turn_id, role, text, state, error_code, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
        defer { sqlite3_finalize(statement) }
        try bind(message.id, to: statement, at: 1)
        try bind(message.turnID, to: statement, at: 2)
        try bind(message.role.rawValue, to: statement, at: 3)
        try bind(message.text, to: statement, at: 4)
        try bind(message.state.rawValue, to: statement, at: 5)
        try bind(message.errorCode, to: statement, at: 6)
        sqlite3_bind_double(statement, 7, message.createdAt.timeIntervalSince1970)
        guard sqlite3_step(statement) == SQLITE_DONE else { throw CloudChatStoreError.damagedStore }
    }

    private func prepare(_ db: OpaquePointer, _ sql: String) throws -> OpaquePointer {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
            throw CloudChatStoreError.damagedStore
        }
        return statement
    }

    private func bind(_ text: String?, to statement: OpaquePointer, at index: Int32) throws {
        let result: Int32
        if let text { result = text.withCString { sqlite3_bind_text(statement, index, $0, -1, transient) } }
        else { result = sqlite3_bind_null(statement, index) }
        guard result == SQLITE_OK else { throw CloudChatStoreError.damagedStore }
    }

    private func read(_ statement: OpaquePointer) throws -> CloudChatMessage {
        func string(_ index: Int32) -> String? {
            sqlite3_column_text(statement, index).map { String(cString: $0) }
        }
        guard let id = string(0), UUID(uuidString: id) != nil,
              let turnID = string(1), UUID(uuidString: turnID) != nil,
              let roleText = string(2), let role = CloudChatRole(rawValue: roleText),
              let text = string(3), let stateText = string(4), let state = CloudChatState(rawValue: stateText) else {
            throw CloudChatStoreError.damagedStore
        }
        let timestamp = sqlite3_column_double(statement, 6)
        guard timestamp.isFinite else { throw CloudChatStoreError.damagedStore }
        return CloudChatMessage(id: id, turnID: turnID, role: role, text: text,
                                state: state, errorCode: string(5),
                                createdAt: Date(timeIntervalSince1970: timestamp))
    }
}
