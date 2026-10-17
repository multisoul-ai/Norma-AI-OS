import E2EKit
import Foundation

public enum ProjectAreaFixture {
    /// 在隔离数据库写入一个项目区，并在项目目录留下 keep-me.txt。调用时 App 不得占用数据库。
    public static func install(dataRoot: URL, workspaceName: String) throws -> URL {
        let project = dataRoot.appendingPathComponent("project-fixture", isDirectory: true)
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        let sentinel = project.appendingPathComponent("keep-me.txt")
        try Data("Project files must survive workspace deletion.\n".utf8).write(to: sentinel)
        try Catalog.seedArea(
            database: dataRoot.appendingPathComponent("app.sqlite"),
            workspaceName: workspaceName,
            root: project,
            displayName: "E2E Project"
        )
        return sentinel
    }
}
