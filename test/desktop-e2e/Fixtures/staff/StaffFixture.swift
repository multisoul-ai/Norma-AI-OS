import Foundation
import E2EKit

public enum StaffFixture {
    public static func install() throws -> URL {
        let root = URL(fileURLWithPath: "/private/tmp").appendingPathComponent("norma-staff-\(UUID().uuidString)")
        for directory in ["bin", "adapters/node_modules/.bin"] {
            try FileManager.default.createDirectory(at: root.appendingPathComponent(directory), withIntermediateDirectories: true)
        }
        try Data(#"{"enabled":true,"backend":"claude","strategy":"speed","speak":false,"summary":"off","autonomy":{"assign_followups":false}}"#.utf8).write(to: root.appendingPathComponent("config.json"))
        try Data("export PATH=\"\(root.path)/bin:$PATH\"\n".utf8).write(to: root.appendingPathComponent(".zshrc"))
        for (path, script) in [("bin/claude", StaffWorker.script), ("bin/opencode", StaffOpenCode.script), ("adapters/node_modules/.bin/claude-agent-acp", StaffAdapter.script)] {
            let file = root.appendingPathComponent(path)
            try Data(script.utf8).write(to: file)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: file.path)
        }
        return root
    }

    public static func tasks(_ root: URL) throws -> [[String: Any]] {
        let files = (FileManager.default.enumerator(at: root.appendingPathComponent("projects"), includingPropertiesForKeys: nil)?.allObjects as? [URL] ?? [])
            .filter { $0.lastPathComponent == "tasks.json" }
        return try files.flatMap { file -> [[String: Any]] in
            let data = try Data(contentsOf: file)
            guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any], let tasks = object["tasks"] as? [[String: Any]] else {
                throw E2EError.product("Invalid TaskStore snapshot")
            }
            return tasks
        }
    }

    public static func records(_ root: URL, _ name: String) throws -> [[String: Any]] {
        let text = (try? String(contentsOf: root.appendingPathComponent(name), encoding: .utf8)) ?? ""
        return try text.split(separator: "\n").map {
            guard let record = try JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] else { throw E2EError.product("Invalid fixture record") }
            return record
        }
    }
}
