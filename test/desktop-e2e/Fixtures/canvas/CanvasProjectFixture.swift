import E2EKit
import Foundation

public enum CanvasProjectFixture {
    /// 项目需位于仓库外，才能让真实 App 的项目身份校验得到项目自身路径。
    public static func install(dataRoot: URL, workspaceName: String) throws -> URL {
        let project = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("norma-canvas-e2e-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        let root = project
        try Data("E2E canvas project\n".utf8)
            .write(to: root.appendingPathComponent("README.md"))
        try Catalog.seedArea(
            database: dataRoot.appendingPathComponent("app.sqlite"),
            workspaceName: workspaceName,
            root: root,
            displayName: "E2E Project"
        )
        return root
    }
}
