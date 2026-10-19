import E2EKit
import Foundation

public enum ContentFixture {
    public static func install(dataRoot: URL, workspace: String) throws -> URL {
        let root = URL(fileURLWithPath: "/private/tmp/norma-content-e2e-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data("ORIGINAL FILE CONTENT\n".utf8).write(to: root.appendingPathComponent("NoteE2E.txt"))
        try Data("# MARKDOWN PREVIEW E2E\n\n| A | B |\n|---|---|\n| one | two |\n".utf8).write(to: root.appendingPathComponent("PlanE2E.md"))
        try Data("<!doctype html><h1>HTML PREVIEW E2E</h1>".utf8).write(to: root.appendingPathComponent("PageE2E.html"))
        try ContentMediaFixture.install(root)
        try git(root, ["init", "-b", "main"])
        try git(root, ["config", "user.name", "Norma E2E"])
        try git(root, ["config", "user.email", "norma-e2e@example.invalid"])
        try git(root, ["add", "."])
        try git(root, ["commit", "-m", "fixture"])
        try Data("LOCAL CHANGE E2E\n".utf8).write(to: root.appendingPathComponent("NoteE2E.txt"))
        try Catalog.seedArea(database: dataRoot.appendingPathComponent("app.sqlite"), workspaceName: workspace, root: root, displayName: "Content Project")
        try Catalog.seedContentTab(database: dataRoot.appendingPathComponent("app.sqlite"), workspaceName: workspace,
            sourceAreaName: "Content Project", kind: "project_files", resource: "project-files", title: "文件", position: 0, selected: true)
        return root
    }

    public static func gitOutput(_ root: URL, _ args: [String]) throws -> String {
        let process = Process()
        let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = args
        process.currentDirectoryURL = root
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        try process.run()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw E2EError.product("Git evidence failed") }
        return String(decoding: data, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    public static func git(_ root: URL, _ args: [String]) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = args
        process.currentDirectoryURL = root
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw E2EError.product("Git fixture failed: \(args)") }
    }
}
