import E2EKit
import Foundation

/// Use the installed OpenCode binary and production lifecycle plugin in an isolated HOME.
@MainActor
public final class OpenCodeStatusFixture {
    public let home: URL
    private let root: URL
    private let model = Process()
    private let log: FileHandle
    private var previousEnvironment: [String: String?] = [:]

    public init(root: URL) throws {
        self.root = root
        home = root.appendingPathComponent("home")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let logURL = root.appendingPathComponent("fixture.log")
        _ = FileManager.default.createFile(atPath: logURL.path, contents: nil)
        log = try FileHandle(forWritingTo: logURL)
    }

    public func start(appPath: String) async throws {
        let environment = ProcessInfo.processInfo.environment
        let candidates = (environment["PATH"] ?? "").split(separator: ":").map { "\($0)/opencode" }
        guard let binary = environment["OPENCODE_BIN"] ?? candidates.first(where: { FileManager.default.isExecutableFile(atPath: $0) }),
              binary.hasPrefix("/"), FileManager.default.isExecutableFile(atPath: binary) else {
            throw E2EError.env("需要安装 OpenCode 1.19.3 或 2.x，将其加入 PATH 或用 OPENCODE_BIN 指定绝对路径")
        }

        var repository = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { repository.deleteLastPathComponent() }
        let script = repository.appendingPathComponent("crates/runtime/tests/support/questions/opencode_model.py")
        model.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        model.arguments = ["-I", script.path, root.path, "--desktop-e2e"]
        model.standardOutput = log
        model.standardError = log
        try model.run()
        let portFile = try await waitFor("port", timeout: 5, startup: true)
        let port = try String(contentsOf: portFile, encoding: .utf8)
        let config = """
        {"model":"fixture/question","small_model":"fixture/question","permission":{"*":"deny","question":"allow"},
        "provider":{"fixture":{"name":"Local fixture","npm":"@ai-sdk/openai-compatible",
        "options":{"baseURL":"http://127.0.0.1:\(port)/v1","apiKey":"local-fixture"},
        "models":{"question":{"name":"Question fixture","limit":{"context":32000,"output":4096}}}}}}
        """
        // App startup also probes OpenCode 2.x through ACP. Only the card's login shell
        // should override the database path for OpenCode 1.19.3.
        let database = home.appendingPathComponent("card.db").path.replacingOccurrences(of: "'", with: "'\\''")
        try "export OPENCODE_DB='\(database)'\n".write(
            to: home.appendingPathComponent(".zshenv"), atomically: true, encoding: .utf8
        )
        let overrides = [
            "ZDOTDIR": home.path, "XDG_CONFIG_HOME": home.appendingPathComponent(".config").path,
            "XDG_DATA_HOME": home.appendingPathComponent("data").path,
            "XDG_CACHE_HOME": home.appendingPathComponent("cache").path,
            "XDG_STATE_HOME": home.appendingPathComponent("state").path,
            "OPENCODE_BIN": binary,
            "OPENCODE_CONFIG_CONTENT": config, "OPENCODE_DISABLE_AUTOUPDATE": "1",
            "OPENCODE_DISABLE_MODELS_FETCH": "1", "OPENCODE_DISABLE_DEFAULT_PLUGINS": "1",
            "OPENCODE_DISABLE_PROJECT_CONFIG": "1",
        ]
        for key in Set(overrides.keys).union(environment.keys.filter({ $0.hasPrefix("OPENCODE_") })) {
            previousEnvironment.updateValue(environment[key], forKey: key)
            if let value = overrides[key] { setenv(key, value, 1) } else { unsetenv(key) }
        }

        let version = try run(binary, ["--version"]).trimmingCharacters(in: .whitespacesAndNewlines)
        let core = version.split(separator: " ").last.map(String.init) ?? version
        let normalized = core.hasPrefix("v") ? String(core.dropFirst()) : core
        guard normalized == "1.19.3" || normalized.hasPrefix("2.") else {
            throw E2EError.env("需要 OpenCode 1.19.3 或 2.x，实际是 \(version)")
        }
        let hook = URL(fileURLWithPath: appPath).deletingLastPathComponent().appendingPathComponent("norma-hook").path
        _ = try run(hook, ["install", "--target", "opencode"])
        // Fill OpenCode's cold plugin cache before the card's bounded session-create request.
        // 2.x 的 debug 会去连共享服务；用私有服务，避免占用用户已经打开的 49374。
        _ = try run("/bin/zsh", ["-c", #"exec "$OPENCODE_BIN" --standalone debug config"#])
        try Data("\(binary) \(version)\n".utf8).write(to: root.appendingPathComponent("opencode-version.txt"))
    }

    public func waitFor(_ name: String, timeout: Double = 35, startup: Bool = false) async throws -> URL {
        let file = root.appendingPathComponent(name)
        let deadline = Date().addingTimeInterval(timeout)
        while !FileManager.default.fileExists(atPath: file.path) {
            if !model.isRunning { throw E2EError.env("本机模型已退出，查看 \(logPath)") }
            guard Date() < deadline else {
                let message = "未收到 OpenCode 模型证据 \(name)，查看 \(logPath)"
                throw startup ? E2EError.env(message) : E2EError.product(message)
            }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        return file
    }

    public func release(_ phase: String) throws {
        try Data().write(to: root.appendingPathComponent("release-\(phase)"))
    }

    public func requireGreenAnswer() async throws {
        let receipt = try await waitFor("consumed.json")
        let answers = try JSONSerialization.jsonObject(with: Data(contentsOf: receipt)) as? [[String: Any]] ?? []
        guard answers.count == 1, let answer = answers.first,
              answer["tool_call_id"] as? String == "native_fixture_question",
              let content = answer["content"] as? String,
              content.contains(#""Which fixture color should I use?"="Green""#), !content.contains("Blue") else {
            throw E2EError.product("真实 OpenCode 的后续模型请求应只携带一次 Green 工具答案，查看 \(receipt.path)")
        }
    }

    public func stop() {
        if model.isRunning { model.terminate(); model.waitUntilExit() }
        try? log.close()
        for (key, value) in previousEnvironment {
            if let value { setenv(key, value, 1) } else { unsetenv(key) }
        }
        previousEnvironment.removeAll()
    }

    private var logPath: String { root.appendingPathComponent("fixture.log").path }

    private func run(_ path: String, _ arguments: [String]) throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: path)
        process.arguments = arguments
        process.currentDirectoryURL = home
        var environment = ProcessInfo.processInfo.environment
        environment["HOME"] = home.path
        process.environment = environment
        let output = Pipe()
        process.standardOutput = output
        process.standardError = log
        do { try process.run() } catch { throw E2EError.env("无法启动 \(path)：\(error)") }
        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { throw E2EError.env("\(path) 执行失败，查看 \(logPath)") }
        return String(decoding: data, as: UTF8.self)
    }
}
