import E2EKit
import Foundation

public struct ContentTabControlsFixture {
    public static let workspace = "Drawer Tab Controls"
    public static let project = "E2E Project"
    public let root: URL
    public let webState: URL
    public let start: URL
    public let alpha: URL
    public let zulu: URL
    public let middle: URL

    public init(dataRoot: URL) {
        // Keep the fixed project prefix short so both unique path tokens can be seen in the narrow list.
        root = URL(fileURLWithPath: "/private/tmp", isDirectory: true)
            .appendingPathComponent("tab-\(UUID().uuidString.prefix(8))", isDirectory: true)
        webState = dataRoot.appendingPathComponent("tab-controls-http", isDirectory: true)
        start = root.appendingPathComponent("First.txt")
        middle = root.appendingPathComponent("Spare.txt")
        let suffix = "quarterly-acceptance/notes-and-final-delivery/archived-review/Plan.txt"
        alpha = root.appendingPathComponent("alpha-review/\(suffix)")
        zulu = root.appendingPathComponent("zulu-review/\(suffix)")
    }

    /// Seed only before launch. Exact resources and order are used as the SQLite oracle after user input.
    public func install(database: URL, server: ContentBrowserFixture, many: Bool) throws -> [String] {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try write("FILE BEFORE", to: start)
        try write("ALPHA FILE", to: alpha)
        try write("ZULU FILE", to: zulu)
        try Catalog.seedWorkspace(database: database, name: Self.workspace)
        try Catalog.suppressFeatureGuide(database: database)
        try VoiceConfigurationFixture.theme(database: database, id: many ? "ink-black" : "sapphire-night")
        try Catalog.seedArea(database: database, workspaceName: Self.workspace, root: root, displayName: Self.project)
        var resources: [String] = []
        func seed(_ title: String, _ resource: String, kind: String = "file", selected: Bool = false) throws {
            try Catalog.seedContentTab(database: database, workspaceName: Self.workspace,
                sourceAreaName: Self.project, kind: kind, resource: resource, title: title,
                position: resources.count, selected: selected)
            resources.append(resource)
        }
        try seed("First.txt", start.path, selected: true)
        if many {
            try seed("Plan.txt", alpha.path)
            let localAddress = server.address.replacingOccurrences(of: "127.0.0.1", with: "localhost")
            for index in 3...22 {
                if index.isMultiple(of: 2) {
                    try seed("Web \(index)", "\(localAddress)?tab=\(index)", kind: "browser")
                } else {
                    let file = root.appendingPathComponent("Notes\(index).txt")
                    try write("NOTES \(index)", to: file)
                    try seed(file.lastPathComponent, file.path)
                }
            }
        }
        try seed("Plan.txt", zulu.path)
        if !many {
            try write("SPARE FILE", to: middle)
            try seed(middle.lastPathComponent, middle.path)
        }
        try seed("Loopback", server.refreshAddress, kind: "browser")
        return resources
    }

    public func write(_ text: String, to file: URL) throws {
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("\(text)\n".utf8).write(to: file, options: .atomic)
    }

    public var requestCount: Int {
        let requests = try? String(contentsOf: webState.appendingPathComponent("requests.log"), encoding: .utf8)
        return requests?.split(separator: "\n").filter { $0 == "/refresh" }.count ?? 0
    }
}
