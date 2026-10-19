import E2EKit
import Foundation
import SQLite3

/// Uses only the session's isolated database; the private key never appears in fixture source or output.
public enum LiveCredentialFixture {
    public static func install(database: URL, keyFile: URL) throws {
        let key = try String(contentsOf: keyFile, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines)
        guard !key.isEmpty else { throw E2EError.env("测试Key文件为空") }
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: database.deletingLastPathComponent().path)
        var raw: OpaquePointer?
        guard sqlite3_open_v2(database.path, &raw, SQLITE_OPEN_READWRITE, nil) == SQLITE_OK, let db = raw else {
            if let raw { sqlite3_close(raw) }
            throw E2EError.product("隔离数据库不可用")
        }
        defer { sqlite3_close(db) }
        for (name, value) in [
            ("voice.desktop.mode", "gpt_live"),
            ("voice.gpt_live.credential_ref", "gpt-live-desktop-e2e"),
            ("credential.agent-provider.gpt-live-desktop-e2e", key),
        ] {
            var statement: OpaquePointer?
            let sql = "INSERT INTO settings(key,value,updated_at_unix) VALUES(?1,?2,0) ON CONFLICT(key) DO UPDATE SET value=excluded.value"
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else { throw E2EError.product("隔离凭据准备失败") }
            defer { sqlite3_finalize(statement) }
            let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
            _ = name.withCString { sqlite3_bind_text(statement, 1, $0, -1, transient) }
            _ = value.withCString { sqlite3_bind_text(statement, 2, $0, -1, transient) }
            guard sqlite3_step(statement) == SQLITE_DONE else { throw E2EError.product("隔离凭据保存失败") }
        }
    }
}
