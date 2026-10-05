import Foundation
import SQLite3

/// Phone-owned history and outbox. No cloud database or legacy thread IDs.
public actor WorkspaceChatStore {
    private let databaseURL: URL
    private let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
    public init(databaseURL: URL) { self.databaseURL = databaseURL }

    public func catalog(macID: String) throws -> WorkspaceCatalog? {
        try withDatabase { try read(WorkspaceCatalog.self, kind: "catalog", key: macID, db: $0).first }
    }
    public func saveCreation(_ entry: PendingAgentCreation) throws {
        try entry.request.validate()
        guard validID(entry.macID) else { throw AgentCreationError.invalid }
        try entry.receipt?.validate(entry.request, macID: entry.macID)
        try withDatabase { try put(entry, kind: "agent_creation", key: entry.request.operationID, macID: entry.macID, workspaceID: entry.request.workspaceID, db: $0) }
    }
    public func pendingCreation(macID: String, workspaceID: String, areaID: String) throws -> PendingAgentCreation? {
        try withDatabase { db in
            try read(PendingAgentCreation.self, kind: "agent_creation", macID: macID, workspaceID: workspaceID, db: db)
                .first { $0.request.areaID == areaID && $0.receipt?.completed != true && $0.rejection == nil }
        }
    }
    public func saveCatalog(_ catalog: WorkspaceCatalog) throws {
        guard validID(catalog.macID), catalog.workspaces.count <= 256,
              Set(catalog.workspaces.map(\.id)).count == catalog.workspaces.count,
              catalog.workspaces.allSatisfy({ validID($0.id) && !$0.name.isEmpty && $0.revision > 0 }) else { throw WorkspaceChatError.invalid }
        try withDatabase(transaction: true) { db in
            for space in catalog.workspaces { try put(space, kind: "workspace", key: scope(catalog.macID,space.id), macID: catalog.macID, workspaceID: space.id, db: db) }
            try put(catalog, kind: "catalog", key: catalog.macID, macID: catalog.macID, workspaceID: "", db: db)
        }
    }

    public func historicalWorkspaces(macID: String) throws -> [DesktopChatWorkspace] {
        try withDatabase { try read(DesktopChatWorkspace.self, kind: "workspace", macID: macID, db: $0) }
    }

    public func enqueue(_ message: WorkspaceChatMessage, macID: String) throws {
        try message.validate()
        guard validID(macID) else { throw WorkspaceChatError.invalid }
        try withDatabase(transaction: true) { db in
            if let existing = try read(WorkspaceOutboxEntry.self, kind: "outbox", key: message.operationID, db: db).first {
                guard existing.macID == macID, existing.message == message else { throw WorkspaceChatError.conflict }
                return
            }
            try put(WorkspaceOutboxEntry(macID: macID, message: message), kind: "outbox", key: message.operationID,
                    macID: macID, workspaceID: message.workspaceID, db: db)
            try put(WorkspaceChatDraft(), kind: "draft", key: scope(macID, message.workspaceID), macID: macID, workspaceID: message.workspaceID, db: db)
        }
    }
    public func pending(macID: String) throws -> [WorkspaceOutboxEntry] {
        try outbox(macID: macID).filter { $0.receipt == nil || $0.receipt?.state == "received" }
    }
    public func outbox(macID: String, workspaceID: String? = nil) throws -> [WorkspaceOutboxEntry] {
        try withDatabase { try read(WorkspaceOutboxEntry.self, kind: "outbox", macID: macID, workspaceID: workspaceID, db: $0) }
    }
    public func apply(_ receipt: WorkspaceChatReceipt) throws {
        try withDatabase(transaction: true) { db in
            guard var entry = try read(WorkspaceOutboxEntry.self, kind: "outbox", key: receipt.operationID, db: db).first,
                  entry.macID == receipt.macID, entry.message.workspaceID == receipt.workspaceID,
                  receipt.cursor > 0, ["received", "processing", "succeeded", "failed", "outcome_unknown"].contains(receipt.state) else { throw WorkspaceChatError.invalid }
            if let previous = entry.receipt, previous.cursor > receipt.cursor { return }
            entry.receipt = receipt
            try put(entry, kind: "outbox", key: entry.id, macID: entry.macID, workspaceID: entry.message.workspaceID, db: db)
        }
    }

    public func events(macID: String, workspaceID: String) throws -> [WorkspaceChatEvent] {
        try withDatabase { try read(WorkspaceChatEvent.self, kind: "event", macID: macID, workspaceID: workspaceID, db: $0).sorted { $0.cursor < $1.cursor } }
    }
    public func cursor(macID: String, workspaceID: String) throws -> UInt64 {
        try withDatabase { try read(UInt64.self, kind: "cursor", key: scope(macID, workspaceID), db: $0).first ?? 0 }
    }
    public func applyEvents(_ events: [WorkspaceChatEvent], macID: String, workspaceID: String, after: UInt64) throws {
        guard events.count <= 32, validID(macID), validID(workspaceID) else { throw WorkspaceChatError.invalid }
        var next = after
        for event in events {
            guard event.macID == macID, event.workspaceID == workspaceID, event.cursor > next, event.cursor <= Int64.max, validID(event.operationID), validID(event.deviceID),
                  ["received", "submitted", "dispatch", "running", "succeeded", "failed", "outcome_unknown", "task_result"].contains(event.kind) else { throw WorkspaceChatError.invalid }
            next = event.cursor
        }
        let last = next
        try withDatabase(transaction: true) { db in
            let saved = try read(UInt64.self, kind: "cursor", key: scope(macID, workspaceID), db: db).first ?? 0
            guard saved == after else { throw WorkspaceChatError.conflict }
            for event in events {
                let key = scope(macID, workspaceID) + ":" + String(event.cursor)
                if let existing = try read(WorkspaceChatEvent.self, kind: "event", key: key, db: db).first {
                    guard existing == event else { throw WorkspaceChatError.invalid }
                } else { try put(event, kind: "event", key: key, macID: macID, workspaceID: workspaceID, db: db) }
            }
            // A final event advances the local outbox truth in the same transaction as the cursor.
            for event in events where ["succeeded", "failed", "outcome_unknown"].contains(event.kind) {
                if var entry = try read(WorkspaceOutboxEntry.self, kind: "outbox", key: event.operationID, db: db).first,
                   entry.macID == macID, entry.message.workspaceID == workspaceID,
                   entry.receipt == nil || entry.receipt?.deviceID == event.deviceID {
                    entry.receipt = WorkspaceChatReceipt(deviceID: event.deviceID, macID: macID, workspaceID: workspaceID,
                                                         operationID: event.operationID, state: event.kind, cursor: event.cursor, result: event.payload)
                    try put(entry, kind: "outbox", key: entry.id, macID: macID, workspaceID: workspaceID, db: db)
                }
            }
            try put(last, kind: "cursor", key: scope(macID, workspaceID), macID: macID, workspaceID: workspaceID, db: db)
        }
    }
    public func saveDraft(_ text: String, macID: String, workspaceID: String) throws {
        var draft = try draftState(macID: macID, workspaceID: workspaceID)
        draft.text = text
        try saveDraft(draft, macID: macID, workspaceID: workspaceID)
    }
    public func saveDraft(_ draft: WorkspaceChatDraft, macID: String, workspaceID: String) throws {
        guard validID(macID), validID(workspaceID), draft.text.utf8.count <= 100_000 else { throw WorkspaceChatError.invalid }
        try withDatabase { try put(draft, kind: "draft", key: scope(macID, workspaceID), macID: macID, workspaceID: workspaceID, db: $0) }
    }
    public func draft(macID: String, workspaceID: String) throws -> String { try draftState(macID: macID, workspaceID: workspaceID).text }
    public func draftState(macID: String, workspaceID: String) throws -> WorkspaceChatDraft {
        try withDatabase { db in
            do { return try read(WorkspaceChatDraft.self, kind: "draft", key: scope(macID, workspaceID), db: db).first ?? .init() }
            catch is DecodingError { return WorkspaceChatDraft(text: try read(String.self, kind: "draft", key: scope(macID, workspaceID), db: db).first ?? "") }
        }
    }

    private func scope(_ macID: String, _ workspaceID: String) -> String { macID + ":" + workspaceID }
    private func withDatabase<T>(transaction: Bool = false, _ body: (OpaquePointer) throws -> T) throws -> T {
        let directory = databaseURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        #if os(iOS)
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: directory.path)
        #endif
        var pointer: OpaquePointer?
        guard sqlite3_open(databaseURL.path, &pointer) == SQLITE_OK, let db = pointer else {
            if let pointer { sqlite3_close(pointer) }; throw WorkspaceChatError.damagedStore
        }
        defer { sqlite3_close(db) }
        // One actor serializes access; 2,000 ms only tolerates another process reading the test DB.
        sqlite3_busy_timeout(db, 2_000)
        try execute(db, "CREATE TABLE IF NOT EXISTS workspace_chat_records(ordinal INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT NOT NULL,key TEXT NOT NULL,mac_id TEXT NOT NULL,workspace_id TEXT NOT NULL,payload BLOB NOT NULL,UNIQUE(kind,key))")
        #if os(iOS)
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: databaseURL.path)
        #endif
        if transaction { try execute(db, "BEGIN IMMEDIATE") }
        do { let result = try body(db); if transaction { try execute(db, "COMMIT") }; return result }
        catch { if transaction { try? execute(db, "ROLLBACK") }; throw error }
    }
    private func execute(_ db: OpaquePointer, _ sql: String) throws {
        guard sqlite3_exec(db, sql, nil, nil, nil) == SQLITE_OK else { throw WorkspaceChatError.damagedStore }
    }
    private func statement(_ db: OpaquePointer, _ sql: String) throws -> OpaquePointer {
        var pointer: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &pointer, nil) == SQLITE_OK, let pointer else { throw WorkspaceChatError.damagedStore }
        return pointer
    }
    private func bind(_ statement: OpaquePointer, _ values: [String]) throws {
        for (index,value) in values.enumerated() {
            guard sqlite3_bind_text(statement, Int32(index + 1), value, -1, transient) == SQLITE_OK else { throw WorkspaceChatError.damagedStore }
        }
    }
    private func put<T: Encodable>(_ value: T, kind: String, key: String, macID: String, workspaceID: String, db: OpaquePointer) throws {
        let data = try JSONEncoder().encode(value)
        let query = try statement(db, "INSERT INTO workspace_chat_records(kind,key,mac_id,workspace_id,payload) VALUES(?,?,?,?,?) ON CONFLICT(kind,key) DO UPDATE SET payload=excluded.payload")
        defer { sqlite3_finalize(query) }
        try bind(query, [kind,key,macID,workspaceID])
        let result = Self.bindPayload(data, to: query)
        guard result == SQLITE_OK, sqlite3_step(query) == SQLITE_DONE else { throw WorkspaceChatError.damagedStore }
    }
    nonisolated private static func bindPayload(_ data: Data, to query: OpaquePointer) -> Int32 {
        let destructor = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
        return data.withUnsafeBytes { sqlite3_bind_blob(query, 5, $0.baseAddress, Int32($0.count), destructor) }
    }
    private func read<T: Decodable>(_ type: T.Type, kind: String, key: String? = nil, macID: String? = nil, workspaceID: String? = nil, db: OpaquePointer) throws -> [T] {
        var sql = "SELECT payload FROM workspace_chat_records WHERE kind=?"
        var values = [kind]
        if let key { sql += " AND key=?"; values.append(key) }
        if let macID { sql += " AND mac_id=?"; values.append(macID) }
        if let workspaceID { sql += " AND workspace_id=?"; values.append(workspaceID) }
        let query = try statement(db, sql + " ORDER BY ordinal")
        defer { sqlite3_finalize(query) }
        try bind(query, values)
        var result: [T] = []
        while true {
            let status = sqlite3_step(query)
            if status == SQLITE_DONE { return result }
            guard status == SQLITE_ROW, let bytes = sqlite3_column_blob(query, 0) else { throw WorkspaceChatError.damagedStore }
            let data = Data(bytes: bytes, count: Int(sqlite3_column_bytes(query, 0)))
            result.append(try JSONDecoder().decode(T.self, from: data))
        }
    }
}
