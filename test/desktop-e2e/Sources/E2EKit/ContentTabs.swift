import Foundation
import SQLite3

public struct ContentTabRow: Equatable, CustomStringConvertible {
    public let id: String
    public let title: String
    public let kind: String
    public let selected: Bool
    public let sourceRemoved: Bool

    public var description: String {
        "\(title)(\(kind), selected=\(selected), sourceRemoved=\(sourceRemoved))"
    }
}

extension Session {
    public func contentTabs(workspaceName: String) throws -> [ContentTabRow] {
        try Catalog.contentTabs(database: databaseURL(), workspaceName: workspaceName)
    }
}

extension Catalog {
    public static func seedContentTab(
        database url: URL,
        workspaceName: String,
        sourceAreaName: String,
        kind: String,
        resource: String,
        title: String,
        position: Int,
        selected: Bool
    ) throws {
        guard kind == "file" || kind == "browser" else {
            throw E2EError.product("内容标签夹具类型无效：\(kind)")
        }
        let database = try SQLiteDB(path: url.path, write: true)
        defer { database.close() }
        let areas = try database.query(
            "SELECT a.id, a.desktop_workspace_id FROM desktop_project_areas a JOIN desktop_workspaces w ON w.id=a.desktop_workspace_id WHERE w.name=?1 AND a.display_name=?2",
            bindings: [workspaceName, sourceAreaName]
        ) { statement in
            (column(statement, 0), column(statement, 1))
        }
        guard let (areaID, workspaceID) = areas.first else {
            throw E2EError.product("内容标签夹具找不到来源项目 \(workspaceName)/\(sourceAreaName)")
        }
        try database.execute(
            "INSERT INTO desktop_content_tabs(id,workspace_id,source_area_id,source_project_name,kind,resource,version,title,position,selected) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            bindings: [
                UUID().uuidString, workspaceID, areaID, sourceAreaName, kind,
                resource, "", title, String(position), selected ? "1" : "0",
            ]
        )
    }

    public static func contentTabs(database url: URL, workspaceName: String) throws -> [ContentTabRow] {
        let database = try SQLiteDB(path: url.path, write: false)
        defer { database.close() }
        return try database.query(
            "SELECT t.id,t.title,t.kind,t.selected,CASE WHEN t.source_area_id IS NULL THEN 0 WHEN a.id IS NULL THEN 1 ELSE 0 END FROM desktop_content_tabs t JOIN desktop_workspaces w ON w.id=t.workspace_id LEFT JOIN desktop_project_areas a ON a.id=t.source_area_id AND a.desktop_workspace_id=t.workspace_id WHERE w.name=?1 ORDER BY t.position,t.id",
            bindings: [workspaceName]
        ) { statement in
            ContentTabRow(
                id: column(statement, 0),
                title: column(statement, 1),
                kind: column(statement, 2),
                selected: sqlite3_column_int(statement, 3) == 1,
                sourceRemoved: sqlite3_column_int(statement, 4) == 1
            )
        }
    }

}
