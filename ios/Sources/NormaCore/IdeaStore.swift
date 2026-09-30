import Foundation
import SQLite3

public enum IdeaKind: String, Sendable { case text, voice }
public enum IdeaTranscriptionState: String, Sendable { case notApplicable = "none", pending, failed, ready }

public struct Idea: Identifiable, Equatable, Sendable {
    public let id: String
    public let kind: IdeaKind
    public let body: String
    public let rawTranscript: String?
    public let transcriptionState: IdeaTranscriptionState
    public let userEdited: Bool
    public let audioFilename: String?
    public let duration: TimeInterval?
    public let createdAt: Date
    public let updatedAt: Date
}

public enum IdeaStoreError: LocalizedError {
    case invalidText, invalidRecording, notFound, damagedStore

    public var errorDescription: String? {
        switch self {
        case .invalidText: "请输入想法内容后再保存。"
        case .invalidRecording: "录音尚未保存，请重试。"
        case .notFound: "找不到这条想法。"
        case .damagedStore: "无法读取想法库；原始数据已保留。"
        }
    }
}

/// App-private SQLite metadata. Audio files stay beside the database, never inside a message or project.
public actor IdeaStore {
    private let databaseURL: URL
    private let recordingsDirectory: URL
    private let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
    private var protectionApplied = false

    public init(databaseURL: URL, recordingsDirectory: URL) {
        self.databaseURL = databaseURL
        self.recordingsDirectory = recordingsDirectory
    }

    public func all() throws -> [Idea] {
        try withDatabase { db in
            let statement = try prepare(db, "SELECT id, kind, body, raw_transcript, transcription_state, audio_filename, duration, created_at, updated_at, body_edited FROM ideas ORDER BY updated_at DESC, id DESC")
            defer { sqlite3_finalize(statement) }
            var result: [Idea] = []
            while true {
                let status = sqlite3_step(statement)
                if status == SQLITE_DONE { return result }
                guard status == SQLITE_ROW else { throw IdeaStoreError.damagedStore }
                result.append(try readIdea(statement))
            }
        }
    }

    public func search(_ query: String) throws -> [Idea] {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !term.isEmpty else { return try all() }
        // ponytail: filter the indexed list in memory; add FTS only when the local library makes this slow.
        return try all().filter { $0.body.localizedStandardContains(term) || ($0.rawTranscript?.localizedStandardContains(term) ?? false) }
    }

    public func createText(_ body: String, at date: Date = .now) throws -> Idea {
        guard !body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, body.utf8.count <= 1_048_576 else {
            throw IdeaStoreError.invalidText
        }
        let idea = Idea(id: UUID().uuidString, kind: .text, body: body, rawTranscript: nil,
                        transcriptionState: .notApplicable, userEdited: false, audioFilename: nil, duration: nil,
                        createdAt: date, updatedAt: date)
        try insert(idea)
        return idea
    }

    public func createVoice(recordingAt source: URL, duration: TimeInterval, transcript: String?,
                            transcriptionComplete: Bool = true, editedBody: String? = nil,
                            userEdited: Bool = false, at date: Date = .now) throws -> Idea {
        guard source.isFileURL, duration.isFinite, duration > 0,
              !userEdited || editedBody != nil,
              let size = (try? FileManager.default.attributesOfItem(atPath: source.path)[.size]) as? NSNumber,
              size.intValue > 0 else { throw IdeaStoreError.invalidRecording }
        let id = UUID().uuidString
        let filename = "\(id).caf"
        try FileManager.default.createDirectory(at: recordingsDirectory, withIntermediateDirectories: true)
        #if os(iOS)
        try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: recordingsDirectory.path)
        #endif
        let destination = recordingsDirectory.appendingPathComponent(filename)
        try FileManager.default.copyItem(at: source, to: destination)
        do {
            #if os(iOS)
            try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: destination.path)
            #endif
        } catch {
            try? FileManager.default.removeItem(at: destination)
            throw error
        }
        let recognized = transcript.flatMap { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0 }
        let text = editedBody ?? recognized ?? ""
        guard text.utf8.count <= 1_048_576 else {
            try? FileManager.default.removeItem(at: destination)
            throw IdeaStoreError.invalidText
        }
        let idea = Idea(id: id, kind: .voice, body: text, rawTranscript: recognized,
                        transcriptionState: recognized == nil || !transcriptionComplete ? .pending : .ready,
                        userEdited: userEdited, audioFilename: filename,
                        duration: duration, createdAt: date, updatedAt: date)
        do { try insert(idea) }
        catch {
            try? FileManager.default.removeItem(at: destination)
            throw error // The source recording is untouched and may be saved again.
        }
        return idea
    }

    public func updateBody(id: String, body: String, at date: Date = .now) throws -> Idea {
        guard let idea = try find(id) else { throw IdeaStoreError.notFound }
        guard body.utf8.count <= 1_048_576,
              idea.kind == .voice || !body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw IdeaStoreError.invalidText
        }
        return try update(idea, body: body, rawTranscript: idea.rawTranscript,
                          state: idea.transcriptionState, userEdited: idea.kind == .voice || idea.userEdited, at: date)
    }

    public func markTranscriptionFailed(id: String, at date: Date = .now) throws -> Idea {
        guard let idea = try find(id) else { throw IdeaStoreError.notFound }
        guard idea.kind == .voice else { throw IdeaStoreError.invalidRecording }
        return try update(idea, body: idea.body, rawTranscript: idea.rawTranscript,
                          state: .failed, userEdited: idea.userEdited, at: date)
    }

    public func updateTranscript(id: String, transcript: String, at date: Date = .now) throws -> Idea {
        guard let idea = try find(id) else { throw IdeaStoreError.notFound }
        guard idea.kind == .voice, !transcript.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw IdeaStoreError.invalidText
        }
        let body = idea.userEdited ? idea.body : transcript
        return try update(idea, body: body, rawTranscript: transcript,
                          state: .ready, userEdited: idea.userEdited, at: date)
    }

    public func recordingURL(for id: String) throws -> URL? {
        guard let idea = try find(id) else { throw IdeaStoreError.notFound }
        guard let filename = idea.audioFilename else { return nil }
        let url = recordingsDirectory.appendingPathComponent(filename)
        guard FileManager.default.fileExists(atPath: url.path) else { throw IdeaStoreError.invalidRecording }
        return url
    }

    private func find(_ id: String) throws -> Idea? {
        try withDatabase { db in
            let statement = try prepare(db, "SELECT id, kind, body, raw_transcript, transcription_state, audio_filename, duration, created_at, updated_at, body_edited FROM ideas WHERE id = ?")
            defer { sqlite3_finalize(statement) }
            try bind(id, to: statement, at: 1)
            let status = sqlite3_step(statement)
            if status == SQLITE_DONE { return nil }
            guard status == SQLITE_ROW else { throw IdeaStoreError.damagedStore }
            return try readIdea(statement)
        }
    }

    private func insert(_ idea: Idea) throws {
        try withDatabase { db in
            let statement = try prepare(db, "INSERT INTO ideas (id, kind, body, raw_transcript, transcription_state, audio_filename, duration, created_at, updated_at, body_edited) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            defer { sqlite3_finalize(statement) }
            try bind(idea.id, to: statement, at: 1)
            try bind(idea.kind.rawValue, to: statement, at: 2)
            try bind(idea.body, to: statement, at: 3)
            try bind(idea.rawTranscript, to: statement, at: 4)
            try bind(idea.transcriptionState.rawValue, to: statement, at: 5)
            try bind(idea.audioFilename, to: statement, at: 6)
            if let duration = idea.duration { sqlite3_bind_double(statement, 7, duration) }
            else { sqlite3_bind_null(statement, 7) }
            sqlite3_bind_double(statement, 8, idea.createdAt.timeIntervalSince1970)
            sqlite3_bind_double(statement, 9, idea.updatedAt.timeIntervalSince1970)
            sqlite3_bind_int(statement, 10, idea.userEdited ? 1 : 0)
            guard sqlite3_step(statement) == SQLITE_DONE else { throw IdeaStoreError.damagedStore }
        }
    }

    private func update(_ idea: Idea, body: String, rawTranscript: String?, state: IdeaTranscriptionState,
                        userEdited: Bool, at date: Date) throws -> Idea {
        try withDatabase { db in
            let statement = try prepare(db, "UPDATE ideas SET body = ?, raw_transcript = ?, transcription_state = ?, updated_at = ?, body_edited = ? WHERE id = ?")
            defer { sqlite3_finalize(statement) }
            try bind(body, to: statement, at: 1)
            try bind(rawTranscript, to: statement, at: 2)
            try bind(state.rawValue, to: statement, at: 3)
            sqlite3_bind_double(statement, 4, date.timeIntervalSince1970)
            sqlite3_bind_int(statement, 5, userEdited ? 1 : 0)
            try bind(idea.id, to: statement, at: 6)
            guard sqlite3_step(statement) == SQLITE_DONE, sqlite3_changes(db) == 1 else { throw IdeaStoreError.damagedStore }
        }
        return Idea(id: idea.id, kind: idea.kind, body: body, rawTranscript: rawTranscript,
                    transcriptionState: state, userEdited: userEdited,
                    audioFilename: idea.audioFilename, duration: idea.duration,
                    createdAt: idea.createdAt, updatedAt: date)
    }

    private func withDatabase<T>(_ operation: (OpaquePointer) throws -> T) throws -> T {
        try FileManager.default.createDirectory(at: databaseURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        var connection: OpaquePointer?
        let flags = SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX
        guard sqlite3_open_v2(databaseURL.path, &connection, flags, nil) == SQLITE_OK, let db = connection else {
            if let connection { sqlite3_close(connection) }
            throw IdeaStoreError.damagedStore
        }
        defer { sqlite3_close(db) }
        let schema = """
            CREATE TABLE IF NOT EXISTS ideas (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                body TEXT NOT NULL,
                raw_transcript TEXT,
                transcription_state TEXT NOT NULL,
                audio_filename TEXT,
                duration REAL,
                created_at REAL NOT NULL,
                updated_at REAL NOT NULL
            );
            """
        guard sqlite3_exec(db, schema, nil, nil, nil) == SQLITE_OK else { throw IdeaStoreError.damagedStore }
        let version = try prepare(db, "PRAGMA user_version")
        guard sqlite3_step(version) == SQLITE_ROW else {
            sqlite3_finalize(version)
            throw IdeaStoreError.damagedStore
        }
        let schemaVersion = sqlite3_column_int(version, 0)
        sqlite3_finalize(version)
        if schemaVersion == 0 {
            let migration = """
                BEGIN IMMEDIATE;
                ALTER TABLE ideas ADD COLUMN body_edited INTEGER NOT NULL DEFAULT 0;
                UPDATE ideas SET body_edited = 1 WHERE kind = 'voice' AND body <> COALESCE(raw_transcript, '');
                PRAGMA user_version = 1;
                COMMIT;
                """
            guard sqlite3_exec(db, migration, nil, nil, nil) == SQLITE_OK else {
                sqlite3_exec(db, "ROLLBACK", nil, nil, nil)
                throw IdeaStoreError.damagedStore
            }
        }
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

    private func prepare(_ db: OpaquePointer, _ sql: String) throws -> OpaquePointer {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK, let statement else {
            throw IdeaStoreError.damagedStore
        }
        return statement
    }

    private func bind(_ text: String?, to statement: OpaquePointer, at index: Int32) throws {
        let result: Int32
        if let text {
            result = text.withCString { sqlite3_bind_text(statement, index, $0, -1, transient) }
        } else {
            result = sqlite3_bind_null(statement, index)
        }
        guard result == SQLITE_OK else { throw IdeaStoreError.damagedStore }
    }

    private func readIdea(_ statement: OpaquePointer) throws -> Idea {
        func text(_ column: Int32) -> String? {
            sqlite3_column_text(statement, column).map { String(cString: $0) }
        }
        guard let id = text(0), UUID(uuidString: id) != nil,
              let kindString = text(1), let kind = IdeaKind(rawValue: kindString),
              let body = text(2), let stateString = text(4),
              let state = IdeaTranscriptionState(rawValue: stateString) else { throw IdeaStoreError.damagedStore }
        let filename = text(5)
        let duration = sqlite3_column_type(statement, 6) == SQLITE_NULL ? nil : sqlite3_column_double(statement, 6)
        let created = sqlite3_column_double(statement, 7)
        let updated = sqlite3_column_double(statement, 8)
        guard created.isFinite, updated.isFinite,
              (kind == .text && filename == nil && duration == nil && state == .notApplicable) ||
              (kind == .voice && filename == "\(id).caf" && duration.map { $0.isFinite && $0 > 0 } == true && state != .notApplicable) else {
            throw IdeaStoreError.damagedStore
        }
        let edited = sqlite3_column_int(statement, 9)
        guard edited == 0 || edited == 1 else { throw IdeaStoreError.damagedStore }
        return Idea(id: id, kind: kind, body: body, rawTranscript: text(3), transcriptionState: state,
                    userEdited: edited == 1,
                    audioFilename: filename, duration: duration,
                    createdAt: Date(timeIntervalSince1970: created), updatedAt: Date(timeIntervalSince1970: updated))
    }
}
