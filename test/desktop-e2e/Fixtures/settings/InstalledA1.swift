import E2EKit
import Foundation

/// Reuse previously verified real model bytes; never generate model or recognition data.
public enum InstalledA1Fixture {
    public static func clone(into data: URL) throws {
        guard let raw = ProcessInfo.processInfo.environment["NORMA_A1_ACCEPTANCE_INSTALLED_MODELS"] else {
            throw E2EError.env("需要此前真实安装的 models 目录；不使用合成模型或重复下载替代")
        }
        let source = URL(fileURLWithPath: raw).appendingPathComponent("a1").standardizedFileURL
        let target = data.appendingPathComponent("models/a1").standardizedFileURL
        guard let bytes = try? Data(contentsOf: source.appendingPathComponent("runtime.json")),
              var config = try JSONSerialization.jsonObject(with: bytes) as? [String: Any],
              !FileManager.default.fileExists(atPath: target.path) else {
            throw E2EError.env("真实安装源缺失，或隔离目标已存在；拒绝覆盖")
        }
        for field in ["python", "zipformer", "qwen"] {
            guard let path = config[field] as? String, path.hasPrefix(source.path + "/") else {
                throw E2EError.env("真实安装源必须自包含，不能引用外部解释器或模型")
            }
            config[field] = target.path + path.dropFirst(source.path.count)
        }
        try FileManager.default.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
        let copy = Process()
        copy.executableURL = URL(fileURLWithPath: "/bin/cp")
        copy.arguments = ["-cR", source.path, target.path]
        try copy.run()
        copy.waitUntilExit()
        guard copy.terminationStatus == 0 else { throw E2EError.env("无法克隆已验证真实安装") }
        try JSONSerialization.data(withJSONObject: config, options: [.prettyPrinted, .sortedKeys])
            .write(to: target.appendingPathComponent("runtime.json"), options: .atomic)
        guard let python = config["python"] as? String else { throw E2EError.env("缺少受管解释器路径") }
        try JSONEncoder().encode(python).write(to: target.appendingPathComponent("python-runtime.json"), options: .atomic)
    }
}
