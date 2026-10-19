import E2EKit
import Foundation
import SQLite3

public enum LiveAcousticFixture {
    /// A blank workspace is a voice-scene prerequisite, not a workspace CRUD assertion.
    public static func install(database: URL) throws {
        var raw: OpaquePointer?
        guard sqlite3_open_v2(database.path, &raw, SQLITE_OPEN_READWRITE, nil) == SQLITE_OK, let db = raw else {
            if let raw { sqlite3_close(raw) }
            throw E2EError.product("声学夹具数据库不可用")
        }
        defer { sqlite3_close(db) }
        let sql = "INSERT INTO desktop_workspaces(id,name,created_at_unix_ms,last_opened_at_unix_ms) VALUES('e2e-live-acoustic','LiveAcoustic',1,1)"
        guard sqlite3_exec(db, sql, nil, nil, nil) == SQLITE_OK else {
            throw E2EError.product("隔离空白工作空间准备失败")
        }
    }

    public static func expandProject(database: URL) throws {
        var raw: OpaquePointer?
        guard sqlite3_open_v2(database.path, &raw, SQLITE_OPEN_READWRITE, nil) == SQLITE_OK, let db = raw else {
            if let raw { sqlite3_close(raw) }
            throw E2EError.product("声学项目夹具数据库不可用")
        }
        defer { sqlite3_close(db) }
        let sql = "UPDATE desktop_workspaces SET expanded_area_id=(SELECT id FROM desktop_project_areas WHERE desktop_workspace_id='e2e-live-acoustic' LIMIT 1) WHERE id='e2e-live-acoustic'"
        guard sqlite3_exec(db, sql, nil, nil, nil) == SQLITE_OK else {
            throw E2EError.product("隔离项目展开状态准备失败")
        }
    }
}
