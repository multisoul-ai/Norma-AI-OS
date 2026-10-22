import Foundation
import SQLite3

public struct WorkspaceRow: Equatable {
    public let id: String
    public let name: String
}

public struct AreaRow: Equatable {
    public let id: String
    public let workspaceId: String
    public let rootPath: String
    public let displayName: String
}

public struct Catalog: Equatable {
    public var workspaces: [WorkspaceRow]
    public var areas: [AreaRow]

    public var names: [String] { workspaces.map(\.name) }

    public var summary: String {
        let workspace = workspaces.map { "\($0.name)#\($0.id.prefix(8))" }.joined(separator: ",")
        let area = areas.map { "\($0.displayName)->\($0.workspaceId.prefix(8))" }.joined(separator: ",")
        return "workspaces=[\(workspace)] areas=[\(area)]"
    }
}

extension Session {
    public func databaseURL() throws -> URL {
        guard let dataRoot else { throw E2EError.env("数据目录尚未建立") }
        return dataRoot.appendingPathComponent("app.sqlite")
    }

    public func catalog() throws -> Catalog {
        try Catalog.load(database: databaseURL())
    }

    public func setting(_ key: String) throws -> String? {
        let database = try SQLiteDB(path: databaseURL().path, write: false)
        defer { database.close() }
        return try database.query("SELECT value FROM settings WHERE key=?1", bindings: [key]) {
            column($0, 0)
        }.first
    }

    public func waitCatalog(timeout: TimeInterval, _ ready: (Catalog) -> Bool) async throws -> Catalog {
        let deadline = Date().addingTimeInterval(timeout)
        var last = ""
        while Date() < deadline {
            if let catalog = try? catalog() {
                last = catalog.summary
                if ready(catalog) { return catalog }
            }
            try await Task.sleep(nanoseconds: 120_000_000)
        }
        throw E2EError.product("等待 SQLite 超时。最近：\(last)")
    }

    @discardableResult
    public func prove(
        names: [String],
        areaCount: Int,
        linkedTo name: String? = nil
    ) throws -> Catalog {
        let catalog = try catalog()
        guard Set(catalog.names) == Set(names) else {
            throw E2EError.product("desktop_workspaces 名称不符。期望 \(names.sorted())，实际 \(catalog.summary)")
        }
        guard catalog.areas.count == areaCount else {
            throw E2EError.product("desktop_project_areas 数量应为 \(areaCount)，实际 \(catalog.summary)")
        }
        if let name {
            guard let workspace = catalog.workspaces.first(where: { $0.name == name }) else {
                throw E2EError.product("找不到应关联项目区的工作空间 \(name)。\(catalog.summary)")
            }
            guard catalog.areas.allSatisfy({ $0.workspaceId == workspace.id }) else {
                throw E2EError.product("项目区未全部挂在 \(name)。\(catalog.summary)")
            }
        }
        return catalog
    }
}

extension Catalog {
    static func load(database url: URL) throws -> Catalog {
        guard FileManager.default.fileExists(atPath: url.path) else {
            return Catalog(workspaces: [], areas: [])
        }
        let database = try SQLiteDB(path: url.path, write: false)
        defer { database.close() }
        let workspaces = try database.query("SELECT id, name FROM desktop_workspaces ORDER BY created_at_unix_ms, id") { statement in
            WorkspaceRow(id: column(statement, 0), name: column(statement, 1))
        }
        let areas = try database.query(
            "SELECT id, desktop_workspace_id, root_path, display_name FROM desktop_project_areas ORDER BY added_at_unix_ms, id"
        ) { statement in
            AreaRow(id: column(statement, 0), workspaceId: column(statement, 1), rootPath: column(statement, 2), displayName: column(statement, 3))
        }
        return Catalog(workspaces: workspaces, areas: areas)
    }

    /// 仅在被测 App 停止时建立场景前提；创建交互由 workspace 模块独立验收。
    public static func seedWorkspace(database url: URL, name: String) throws {
        let database = try SQLiteDB(path: url.path, write: true)
        defer { database.close() }
        let now = String(Int64(Date().timeIntervalSince1970 * 1000))
        try database.execute(
            "INSERT INTO desktop_workspaces(id,name,created_at_unix_ms,last_opened_at_unix_ms) VALUES(?1,?2,?3,?4)",
            bindings: [UUID().uuidString, name, now, now]
        )
    }

    /// 仅在被测 App 停止时写入「最近打开」，供 `/add` 在不弹系统窗口的情况下列出 repo。
    public static func seedRecent(database url: URL, root: URL, displayName: String) throws {
        let database = try SQLiteDB(path: url.path, write: true)
        defer { database.close() }
        try database.execute(
            "INSERT OR REPLACE INTO recent_workspaces(path,display_name,last_opened_at_unix,source) VALUES(?1,?2,?3,'open')",
            bindings: [root.path, displayName, String(Int64(Date().timeIntervalSince1970))]
        )
    }

    /// 仅在被测 App 停止时写入一条待办想法；列表按 position 排序。
    public static func seedIdea(database url: URL, workspaceName: String, body: String, position: Int) throws {
        let database = try SQLiteDB(path: url.path, write: true)
        defer { database.close() }
        let rows = try database.query("SELECT id FROM desktop_workspaces WHERE name=?1", bindings: [workspaceName]) { statement in
            column(statement, 0)
        }
        guard let workspaceID = rows.first else {
            throw E2EError.product("想法夹具找不到工作空间 \(workspaceName)")
        }
        let now = String(Int64(Date().timeIntervalSince1970))
        try database.execute(
            "INSERT INTO workspace_ideas(id,workspace_id,body,source,state,position,created_at_unix,updated_at_unix) VALUES(?1,?2,?3,'chat','todo',?4,?5,?5)",
            bindings: [UUID().uuidString, workspaceID, body, String(position), now]
        )
    }

    /// 直接写一条应用设置（启动前预置，验证 App 启动后读回）。
    public static func seedSetting(database url: URL, key: String, value: String) throws {
        let database = try SQLiteDB(path: url.path, write: true)
        defer { database.close() }
        let now = String(Int64(Date().timeIntervalSince1970))
        try database.execute(
            "INSERT OR REPLACE INTO settings(key,value,updated_at_unix) VALUES(?1,?2,?3)",
            bindings: [key, value, now]
        )
    }

    /// 想法正文 → 来源（chat / voice / manual）。
    public static func ideaSources(database url: URL) throws -> [String: String] {
        let database = try SQLiteDB(path: url.path, write: false)
        defer { database.close() }
        let rows = try database.query("SELECT body,source FROM workspace_ideas") { statement in
            (column(statement, 0), column(statement, 1))
        }
        return Dictionary(rows, uniquingKeysWith: { first, _ in first })
    }

    /// 读取想法的正文、状态与任务引用，供拖拽投递后的数据断言。
    public static func ideas(database url: URL) throws -> [(body: String, state: String, taskRef: String)] {
        let database = try SQLiteDB(path: url.path, write: false)
        defer { database.close() }
        return try database.query("SELECT body,state,COALESCE(task_ref,'') FROM workspace_ideas ORDER BY position") { statement in
            (column(statement, 0), column(statement, 1), column(statement, 2))
        }
    }

    public static func seedArea(database url: URL, workspaceName: String, root: URL, displayName: String) throws {
        let database = try SQLiteDB(path: url.path, write: true)
        defer { database.close() }
        let rows = try database.query("SELECT id FROM desktop_workspaces WHERE name=?1", bindings: [workspaceName]) { statement in
            column(statement, 0)
        }
        guard let workspaceID = rows.first else {
            throw E2EError.product("项目夹具找不到工作空间 \(workspaceName)")
        }
        try database.execute(
            "INSERT INTO desktop_project_areas(id,desktop_workspace_id,project_key,root_path,display_name,added_at_unix_ms) VALUES(?1,?2,?3,?4,?5,?6)",
            bindings: [
                UUID().uuidString,
                workspaceID,
                root.path,
                root.path,
                displayName,
                String(Int64(Date().timeIntervalSince1970 * 1000)),
            ]
        )
    }
}

func column(_ statement: OpaquePointer?, _ index: Int32) -> String {
    guard let raw = sqlite3_column_text(statement, index) else { return "" }
    return String(cString: raw)
}

final class SQLiteDB {
    private var database: OpaquePointer?

    init(path: String, write: Bool) throws {
        let flags = write ? SQLITE_OPEN_READWRITE : SQLITE_OPEN_READONLY
        guard sqlite3_open_v2(path, &database, flags, nil) == SQLITE_OK else {
            let message = database.map { String(cString: sqlite3_errmsg($0)) } ?? path
            if database != nil { sqlite3_close(database) }
            throw E2EError.product("无法打开隔离数据库 \(path)：\(message)")
        }
        sqlite3_busy_timeout(database, 2000)
    }

    func close() { sqlite3_close(database) }

    func query<Row>(_ sql: String, bindings: [String] = [], map: (OpaquePointer?) -> Row) throws -> [Row] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK else {
            throw E2EError.product("SQLite 准备失败：\(String(cString: sqlite3_errmsg(database)))")
        }
        defer { sqlite3_finalize(statement) }
        try bind(bindings, to: statement)
        var rows: [Row] = []
        while sqlite3_step(statement) == SQLITE_ROW { rows.append(map(statement)) }
        return rows
    }

    func execute(_ sql: String, bindings: [String]) throws {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK else {
            throw E2EError.product("SQLite 写入准备失败：\(String(cString: sqlite3_errmsg(database)))")
        }
        defer { sqlite3_finalize(statement) }
        try bind(bindings, to: statement)
        guard sqlite3_step(statement) == SQLITE_DONE else {
            throw E2EError.product("SQLite 写入失败：\(String(cString: sqlite3_errmsg(database)))")
        }
    }

    private func bind(_ values: [String], to statement: OpaquePointer?) throws {
        let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
        for (offset, value) in values.enumerated() {
            let index = Int32(offset + 1)
            let result = value.withCString { pointer in
                sqlite3_bind_text(statement, index, pointer, -1, transient)
            }
            if result != SQLITE_OK { throw E2EError.product("SQLite 参数绑定失败") }
        }
    }
}
