import Foundation
import SQLite3

public struct WorkThread: Codable, Hashable, Identifiable, Sendable {
    public let id: String
    public let workspaceID: String
    public let title: String
    public let sourceIdeaID: String?
    public let sourceExcerpt: String?
    public let originDeviceID: String
    public let createdAt: Date
}

public enum WorkMessageState: String, Codable, Sendable { case pending, delivered }

public struct WorkThreadDraft: Codable, Sendable, Equatable {
    public let text: String
    public let revision: String
    public let mentions: [WorkspaceAgentReference]?
    public let followupTaskID: String?
    public init(text: String, mentions: [WorkspaceAgentReference] = [], followupTaskID: String? = nil) {
        self.text = text; self.mentions = mentions.isEmpty ? nil : mentions; revision = UUID().uuidString
        self.followupTaskID = followupTaskID
    }
    var valid: Bool {
        let targets = mentions ?? []
        return (!text.isEmpty || followupTaskID != nil) && text.count <= 100_000 && UUID(uuidString: revision) != nil
            && (followupTaskID.map { UUID(uuidString: $0) != nil } ?? true)
            && targets.count <= 256 && Set(targets.map(\.id)).count == targets.count
            && targets.allSatisfy { validID($0.machineID) && validID($0.projectID) && $0.definitionID > 0
                && !$0.name.isEmpty && $0.name.count <= 256 && $0.profile.count <= 256 }
    }
}

public struct WorkThreadMessage: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let workspaceID: String
    public let threadID: String
    public let author: String
    public let originDeviceID: String
    public let text: String
    public let createdAt: Date
    public let state: WorkMessageState
    public var taskOperationID: String? = nil
    public var replyToMessageID: String? = nil
}

public enum WorkspaceFactKind: String, Codable, CaseIterable, Sendable { case goal, decision, fact }

public struct WorkspaceFact: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let workspaceID: String
    public let threadID: String
    public let kind: WorkspaceFactKind
    public let text: String
    public let version: Int
    public let author: String
    public let sourceMessageID: String?
    public let createdAt: Date
}

public enum WorkThreadError: LocalizedError {
    case invalidContent, notFound, archived, conflict, damagedStore
    public var errorDescription: String? {
        switch self {
        case .invalidContent: "内容或来源无效，请检查后重试。"
        case .notFound: "找不到这条工作记录，原有内容已保留。"
        case .archived: "工作空间已归档，请先重开再继续。"
        case .conflict: "这条记录已经变化，请重新读取后再修改。"
        case .damagedStore: "无法读取工作记录，原有数据已保留。"
        }
    }
}

extension CollaborationWorkspaceStore {
    public func messageDrafts() throws -> [String: WorkThreadDraft] {
        try threadDatabase { try readMessageDrafts($0) }
    }

    public func saveMessageDraft(_ draft: WorkThreadDraft?, threadID: String) throws {
        try threadDatabase(writing: true) { db in
            _ = try requireThread(db, threadID)
            _ = try readMessageDrafts(db, threadID: threadID) // Preserve damaged records for recovery.
            if let draft {
                // Match the message ceiling while preserving in-progress whitespace verbatim.
                guard draft.valid else { throw WorkThreadError.invalidContent }
                if let target = draft.followupTaskID {
                    guard try readTaskOutbox(db, operationID: target).task.threadID == threadID else { throw WorkThreadError.invalidContent }
                }
                try insertThreadRecord(db, "INSERT INTO work_thread_drafts(thread_id, payload) VALUES(?, ?) ON CONFLICT(thread_id) DO UPDATE SET payload = excluded.payload", [threadID], draft)
            } else {
                try deleteMessageDraft(db, threadID: threadID)
            }
        }
    }

    private func readMessageDrafts(_ db: OpaquePointer, threadID: String? = nil) throws -> [String: WorkThreadDraft] {
        var drafts: [String: WorkThreadDraft] = [:]
        let sql = "SELECT thread_id, thread_id, payload FROM work_thread_drafts" + (threadID == nil ? "" : " WHERE thread_id = ?")
        for row in try threadRows(db, sql, threadID.map { [$0] } ?? []) {
            _ = try requireThread(db, row.id)
            guard let draft = try? JSONDecoder().decode(WorkThreadDraft.self, from: row.data),
                  draft.valid else { throw WorkThreadError.damagedStore }
            if let target = draft.followupTaskID {
                guard try readTaskOutbox(db, operationID: target).task.threadID == row.id else { throw WorkThreadError.damagedStore }
            }
            drafts[row.id] = draft
        }
        return drafts
    }

    private func deleteMessageDraft(_ db: OpaquePointer, threadID: String) throws {
        let statement = try prepare(db, "DELETE FROM work_thread_drafts WHERE thread_id = ?")
        defer { sqlite3_finalize(statement) }
        try bind(statement, [threadID])
        guard sqlite3_step(statement) == SQLITE_DONE else { throw WorkThreadError.damagedStore }
    }

    public func threads(workspaceID: String) throws -> [WorkThread] {
        try threadDatabase { db in
            try requireWorkspace(db, workspaceID, writing: false)
            return try threadRows(db, "SELECT id, workspace_id, payload FROM work_threads WHERE workspace_id = ? ORDER BY rowid DESC", [workspaceID])
                .map { row in
                    let value = try decode(WorkThread.self, row)
                    guard value.workspaceID == row.scope, UUID(uuidString: value.originDeviceID) != nil else { throw WorkThreadError.damagedStore }
                    return value
                }
        }
    }

    public func createThread(workspaceID: String, title: String, source: Idea? = nil, at date: Date = .now) throws -> WorkThread {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard validText(title, maximum: 256), date.timeIntervalSince1970.isFinite,
              source.map({ UUID(uuidString: $0.id) != nil }) ?? true else { throw WorkThreadError.invalidContent }
        return try threadDatabase(writing: true) { db in
            try requireWorkspace(db, workspaceID, writing: true)
            let record = WorkThread(id: UUID().uuidString, workspaceID: workspaceID, title: title,
                                    sourceIdeaID: source?.id, sourceExcerpt: source?.body,
                                    originDeviceID: try localOrigin(db), createdAt: date)
            try insertThreadRecord(db, "INSERT INTO work_threads(id, workspace_id, payload) VALUES(?, ?, ?)",
                                   [record.id, record.workspaceID], record)
            return record
        }
    }

    // ponytail: reading is linear in the thread's history; add cursor paging when long histories affect scrolling.
    public func messages(threadID: String) throws -> [WorkThreadMessage] {
        try threadDatabase { try messages($0, thread: requireThread($0, threadID)) }
    }

    func messages(_ db: OpaquePointer, thread: WorkThread) throws -> [WorkThreadMessage] {
        try threadRows(db, "SELECT id, workspace_id, payload FROM work_messages WHERE thread_id = ? ORDER BY rowid", [thread.id])
            .map { row in
                let value = try decode(WorkThreadMessage.self, row)
                guard value.workspaceID == row.scope, row.scope == thread.workspaceID, value.threadID == thread.id,
                      validText(value.text) else { throw WorkThreadError.damagedStore }
                try validateMessageAuthor(db, value)
                return value
            }
    }

    public func appendMessage(threadID: String, id: String, text: String, at date: Date = .now,
                              clearingDraftRevision: String? = nil) throws -> WorkThreadMessage {
        try threadDatabase(writing: true) { db in
            try appendMessage(db, threadID: threadID, id: id, text: text, at: date,
                              clearingDraftRevision: clearingDraftRevision)
        }
    }

    func appendMessage(_ db: OpaquePointer, threadID: String, id: String, text: String, at date: Date,
                       clearingDraftRevision: String?) throws -> WorkThreadMessage {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard UUID(uuidString: id) != nil, validText(text), date.timeIntervalSince1970.isFinite else { throw WorkThreadError.invalidContent }
        let thread = try requireThread(db, threadID)
        if let row = try threadRows(db, "SELECT id, workspace_id, payload FROM work_messages WHERE id = ?", [id]).first {
            let original = try decode(WorkThreadMessage.self, row)
            guard original.workspaceID == thread.workspaceID, original.threadID == threadID,
                  original.author == "user", original.taskOperationID == nil, original.text == text else { throw WorkThreadError.conflict }
            _ = try ensureThreadOutbox(db, message: original)
            return original
        }
        try requireWorkspace(db, thread.workspaceID, writing: true)
        let record = WorkThreadMessage(id: id, workspaceID: thread.workspaceID, threadID: threadID, author: "user",
                                       originDeviceID: try localOrigin(db), text: text, createdAt: date, state: .pending)
        try insertThreadRecord(db, "INSERT INTO work_messages(id, workspace_id, thread_id, payload) VALUES(?, ?, ?, ?)",
                               [id, thread.workspaceID, threadID], record)
        _ = try ensureThreadOutbox(db, message: record)
        if let draft = try readMessageDrafts(db, threadID: threadID)[threadID], draft.revision == clearingDraftRevision,
           draft.text.trimmingCharacters(in: .whitespacesAndNewlines) == text {
            try deleteMessageDraft(db, threadID: threadID)
        }
        return record
    }

    public func facts(threadID: String) throws -> [WorkspaceFact] {
        try threadDatabase { db in
            let thread = try requireThread(db, threadID)
            return try currentFacts(db, workspaceID: thread.workspaceID).filter { $0.kind != .goal || $0.threadID == threadID }
        }
    }

    public func sourceMessage(id: String, workspaceID: String) throws -> WorkThreadMessage {
        try threadDatabase { db in
            try requireWorkspace(db, workspaceID, writing: false)
            guard let row = try threadRows(db, "SELECT id, workspace_id, payload FROM work_messages WHERE id = ? AND workspace_id = ?", [id, workspaceID]).first else {
                throw WorkThreadError.notFound
            }
            let message = try decode(WorkThreadMessage.self, row)
            let thread = try requireThread(db, message.threadID)
            guard message.workspaceID == workspaceID, thread.workspaceID == workspaceID,
                  validText(message.text) else { throw WorkThreadError.damagedStore }
            try validateMessageAuthor(db, message)
            return message
        }
    }

    public func factHistory(id: String) throws -> [WorkspaceFact] {
        try threadDatabase { db in
            try threadRows(db, "SELECT id, workspace_id, payload FROM workspace_fact_versions WHERE id = ? ORDER BY version", [id])
                .map { try decode(WorkspaceFact.self, $0) }
        }
    }

    public func saveFact(threadID: String, kind: WorkspaceFactKind, text: String, sourceMessageID: String? = nil,
                         replacing previous: WorkspaceFact? = nil, at date: Date = .now) throws -> WorkspaceFact {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard validText(text), date.timeIntervalSince1970.isFinite else { throw WorkThreadError.invalidContent }
        return try threadDatabase(writing: true) { db in
            let thread = try requireThread(db, threadID)
            try requireWorkspace(db, thread.workspaceID, writing: true)
            let facts = try currentFacts(db, workspaceID: thread.workspaceID)
            if let previous {
                guard previous.workspaceID == thread.workspaceID, previous.kind == kind,
                      facts.first(where: { $0.id == previous.id }) == previous,
                      kind != .goal || previous.threadID == threadID else { throw WorkThreadError.conflict }
            } else if kind == .goal, facts.contains(where: { $0.kind == .goal && $0.threadID == threadID }) {
                throw WorkThreadError.conflict
            }
            if let sourceMessageID {
                guard let source = try threadRows(db, "SELECT id, workspace_id, payload FROM work_messages WHERE id = ?", [sourceMessageID]).first,
                      source.scope == thread.workspaceID else { throw WorkThreadError.invalidContent }
            }
            let record = WorkspaceFact(id: previous?.id ?? UUID().uuidString, workspaceID: thread.workspaceID, threadID: threadID,
                                       kind: kind, text: text, version: (previous?.version ?? 0) + 1, author: "user",
                                       sourceMessageID: sourceMessageID, createdAt: date)
            try insertThreadRecord(db, "INSERT INTO workspace_fact_versions(id, workspace_id, thread_id, version, payload) VALUES(?, ?, ?, ?, ?)",
                                   [record.id, record.workspaceID, record.threadID, String(record.version)], record)
            return record
        }
    }

    func currentFacts(_ db: OpaquePointer, workspaceID: String) throws -> [WorkspaceFact] {
        try threadRows(db, """
            SELECT f.id, f.workspace_id, f.payload FROM workspace_fact_versions f
            WHERE f.workspace_id = ? AND f.version = (SELECT MAX(v.version) FROM workspace_fact_versions v WHERE v.id = f.id)
            ORDER BY f.rowid
            """, [workspaceID]).map { row in
                let fact = try decode(WorkspaceFact.self, row)
                guard fact.workspaceID == row.scope, fact.version > 0, validText(fact.text) else { throw WorkThreadError.damagedStore }
                return fact
            }
    }

    func requireThread(_ db: OpaquePointer, _ id: String) throws -> WorkThread {
        guard let row = try threadRows(db, "SELECT id, workspace_id, payload FROM work_threads WHERE id = ?", [id]).first else {
            throw WorkThreadError.notFound
        }
        let thread = try decode(WorkThread.self, row)
        guard thread.workspaceID == row.scope else { throw WorkThreadError.damagedStore }
        return thread
    }

    func validateMessageAuthor(_ db: OpaquePointer, _ message: WorkThreadMessage) throws {
        if message.author == "user" {
            guard message.taskOperationID == nil, message.replyToMessageID == nil,
                  UUID(uuidString: message.originDeviceID) != nil else { throw WorkThreadError.damagedStore }
        } else if message.author == "norma" {
            guard message.taskOperationID == nil, let record = try readNormaPlan(db, id: message.id),
                  record.response.reply == message.text else { throw WorkThreadError.damagedStore }
        } else {
            guard message.author == "agent",
                  let id = message.taskOperationID else { throw WorkThreadError.damagedStore }
            if message.replyToMessageID != nil {
                try validateFollowupReply(db, message)
                return
            }
            let operation = try readTaskOutbox(db, operationID: id)
            guard operation.replyMessageID == message.id, operation.machineID == message.originDeviceID,
                  operation.task.threadID == message.threadID, operation.task.collaborationWorkspaceID == message.workspaceID,
                  operation.receipt?.execution?.execution.result?.text == message.text,
                  message.state == .delivered else { throw WorkThreadError.damagedStore }
        }
    }

    func requireWorkspace(_ db: OpaquePointer, _ id: String, writing: Bool) throws {
        guard let row = try threadRows(db, "SELECT id, id, configuration FROM collaboration_workspaces WHERE id = ?", [id]).first else {
            throw WorkThreadError.notFound
        }
        let workspace = try decode(CollaborationWorkspace.self, row)
        if writing && workspace.archived { throw WorkThreadError.archived }
    }

    private func localOrigin(_ db: OpaquePointer) throws -> String {
        let insert = try prepare(db, "INSERT OR IGNORE INTO collaboration_origin(slot, id) VALUES(1, ?)")
        defer { sqlite3_finalize(insert) }
        try bind(insert, [UUID().uuidString])
        guard sqlite3_step(insert) == SQLITE_DONE else { throw WorkThreadError.damagedStore }
        let query = try prepare(db, "SELECT id FROM collaboration_origin WHERE slot = 1")
        defer { sqlite3_finalize(query) }
        guard sqlite3_step(query) == SQLITE_ROW, let text = sqlite3_column_text(query, 0),
              UUID(uuidString: String(cString: text)) != nil else { throw WorkThreadError.damagedStore }
        return String(cString: text)
    }

    func threadDatabase<T>(writing: Bool = false, _ operation: (OpaquePointer) throws -> T) throws -> T {
        try withDatabase { db in
            let schema = """
                PRAGMA foreign_keys = ON;
                CREATE TABLE IF NOT EXISTS collaboration_origin(slot INTEGER PRIMARY KEY CHECK(slot = 1), id TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS work_threads(id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES collaboration_workspaces(id), payload BLOB NOT NULL);
                CREATE INDEX IF NOT EXISTS work_threads_scope ON work_threads(workspace_id);
                CREATE TABLE IF NOT EXISTS work_thread_drafts(thread_id TEXT PRIMARY KEY REFERENCES work_threads(id), payload BLOB NOT NULL);
                CREATE TABLE IF NOT EXISTS work_messages(id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES collaboration_workspaces(id), thread_id TEXT NOT NULL REFERENCES work_threads(id), payload BLOB NOT NULL);
                CREATE INDEX IF NOT EXISTS work_messages_scope ON work_messages(thread_id);
                CREATE TABLE IF NOT EXISTS thread_outbox(id TEXT PRIMARY KEY, message_id TEXT NOT NULL UNIQUE REFERENCES work_messages(id), payload BLOB NOT NULL);
                CREATE TABLE IF NOT EXISTS thread_task_outbox(id TEXT PRIMARY KEY, thread_id TEXT NOT NULL REFERENCES work_threads(id), payload BLOB NOT NULL);
                CREATE INDEX IF NOT EXISTS thread_task_outbox_scope ON thread_task_outbox(thread_id);
                CREATE TABLE IF NOT EXISTS thread_followup_outbox(id TEXT PRIMARY KEY, thread_id TEXT NOT NULL REFERENCES work_threads(id), source_message_id TEXT NOT NULL UNIQUE REFERENCES work_messages(id), parent_operation_id TEXT NOT NULL REFERENCES thread_task_outbox(id), payload BLOB NOT NULL);
                CREATE INDEX IF NOT EXISTS thread_followup_outbox_scope ON thread_followup_outbox(thread_id);
                CREATE TABLE IF NOT EXISTS thread_norma_plans(id TEXT PRIMARY KEY, message_id TEXT NOT NULL UNIQUE REFERENCES work_messages(id), thread_id TEXT NOT NULL REFERENCES work_threads(id), payload BLOB NOT NULL);
                CREATE TABLE IF NOT EXISTS workspace_fact_versions(id TEXT NOT NULL, workspace_id TEXT NOT NULL REFERENCES collaboration_workspaces(id), thread_id TEXT NOT NULL REFERENCES work_threads(id), version INTEGER NOT NULL CHECK(version > 0), payload BLOB NOT NULL, PRIMARY KEY(id, version));
                CREATE INDEX IF NOT EXISTS workspace_facts_scope ON workspace_fact_versions(workspace_id);
                """
            guard sqlite3_exec(db, schema, nil, nil, nil) == SQLITE_OK else { throw WorkThreadError.damagedStore }
            if writing && sqlite3_exec(db, "BEGIN IMMEDIATE", nil, nil, nil) != SQLITE_OK { throw WorkThreadError.damagedStore }
            do {
                let result = try operation(db)
                if writing && sqlite3_exec(db, "COMMIT", nil, nil, nil) != SQLITE_OK { throw WorkThreadError.damagedStore }
                return result
            } catch {
                if writing { sqlite3_exec(db, "ROLLBACK", nil, nil, nil) }
                throw error
            }
        }
    }

    func bind(_ statement: OpaquePointer, _ values: [String]) throws {
        for (index, value) in values.enumerated() {
            guard sqlite3_bind_text(statement, Int32(index + 1), value, -1, transient) == SQLITE_OK else { throw WorkThreadError.damagedStore }
        }
    }

    func insertThreadRecord<T: Encodable>(_ db: OpaquePointer, _ sql: String, _ parameters: [String], _ value: T) throws {
        let statement = try prepare(db, sql)
        defer { sqlite3_finalize(statement) }
        try bind(statement, parameters)
        let data = try JSONEncoder().encode(value)
        let column = Int32(parameters.count + 1)
        let destructor = transient
        let bound = data.withUnsafeBytes { sqlite3_bind_blob(statement, column, $0.baseAddress, Int32($0.count), destructor) }
        guard bound == SQLITE_OK, sqlite3_step(statement) == SQLITE_DONE else { throw WorkThreadError.damagedStore }
    }

    func threadRows(_ db: OpaquePointer, _ sql: String, _ parameters: [String]) throws -> [(id: String, scope: String, data: Data)] {
        let statement = try prepare(db, sql)
        defer { sqlite3_finalize(statement) }
        try bind(statement, parameters)
        var rows: [(String, String, Data)] = []
        while true {
            let result = sqlite3_step(statement)
            if result == SQLITE_DONE { return rows }
            guard result == SQLITE_ROW, let id = sqlite3_column_text(statement, 0), let scope = sqlite3_column_text(statement, 1),
                  let bytes = sqlite3_column_blob(statement, 2) else { throw WorkThreadError.damagedStore }
            rows.append((String(cString: id), String(cString: scope), Data(bytes: bytes, count: Int(sqlite3_column_bytes(statement, 2)))))
        }
    }

    func decode<T: Decodable & Identifiable>(_ type: T.Type, _ row: (id: String, scope: String, data: Data)) throws -> T where T.ID == String {
        guard let record = try? JSONDecoder().decode(type, from: row.data), record.id == row.id,
              UUID(uuidString: row.id) != nil else { throw WorkThreadError.damagedStore }
        return record
    }

    private func validText(_ text: String, maximum: Int = 100_000) -> Bool {
        !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && text.count <= maximum
    }
}
