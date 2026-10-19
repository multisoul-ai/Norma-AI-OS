import E2EKit
import Foundation
import SQLite3

/// Only the isolated session database is accepted; never opens the user's AppDB.
public enum VoiceConfigurationFixture {
    public static func value(database: URL, key: String) throws -> String? {
        try withDatabase(database) { db in
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(db, "SELECT value FROM settings WHERE key=?1", -1, &statement, nil) == SQLITE_OK else {
                throw E2EError.product("无法读取隔离设置")
            }
            defer { sqlite3_finalize(statement) }
            bind(key, statement, 1)
            let result = sqlite3_step(statement)
            if result == SQLITE_DONE { return nil }
            guard result == SQLITE_ROW, let text = sqlite3_column_text(statement, 0) else {
                throw E2EError.product("隔离设置读取失败")
            }
            return String(cString: text)
        }
    }

    /// Call only while the isolated app is stopped.
    public static func theme(database: URL, id: String) throws {
        guard ["paper-blue", "sapphire-night"].contains(id) else { throw E2EError.product("未知测试主题") }
        try withDatabase(database) { db in
            var statement: OpaquePointer?
            let sql = "INSERT INTO settings(key,value,updated_at_unix) VALUES('appearance.theme.v1',?1,0) ON CONFLICT(key) DO UPDATE SET value=excluded.value"
            guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else { throw E2EError.product("无法准备主题夹具") }
            defer { sqlite3_finalize(statement) }
            bind(id, statement, 1)
            guard sqlite3_step(statement) == SQLITE_DONE else { throw E2EError.product("主题夹具写入失败") }
        }
    }

    private static func withDatabase<T>(_ url: URL, body: (OpaquePointer) throws -> T) throws -> T {
        var raw: OpaquePointer?
        guard sqlite3_open_v2(url.path, &raw, SQLITE_OPEN_READWRITE, nil) == SQLITE_OK, let db = raw else {
            if let raw { sqlite3_close(raw) }
            throw E2EError.product("隔离设置数据库尚未创建")
        }
        defer { sqlite3_close(db) }
        sqlite3_busy_timeout(db, 2000)
        return try body(db)
    }

    private static func bind(_ value: String, _ statement: OpaquePointer?, _ index: Int32) {
        let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
        _ = value.withCString { sqlite3_bind_text(statement, index, $0, -1, transient) }
    }
}
