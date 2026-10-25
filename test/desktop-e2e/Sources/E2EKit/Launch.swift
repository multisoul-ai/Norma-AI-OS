import AppKit
import Foundation
import VirtualDisplay

/// Keep the Unix socket below macOS's sun_path limit even in a deep worktree.
func isolatedSocketPath(identifier: String) -> String {
    let safe = identifier.filter { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-") }
    return "/tmp/norma-e2e-\(safe.prefix(40)).sock"
}

/// 隔离窗口所在的私有虚拟显示器。驱动进程内只建一次，进程退出时系统回收。
/// `NORMA_E2E_FOREGROUND=1`、`NORMA_E2E_VISIBLE=1` 或 `NORMA_E2E_VIRTUAL_DISPLAY=0` 时不创建。
@MainActor
private var virtualDisplayID: UInt32?

@MainActor
func isolatedDisplayID() -> UInt32? {
    let env = ProcessInfo.processInfo.environment
    if env["NORMA_E2E_FOREGROUND"] == "1" || env["NORMA_E2E_VISIBLE"] == "1" || env["NORMA_E2E_VIRTUAL_DISPLAY"] == "0" {
        return nil
    }
    if let virtualDisplayID { return virtualDisplayID }
    let id = NormaE2ECreateVirtualDisplay(1440, 900)
    guard id != 0 else {
        print("虚拟显示器不可用，隔离窗口改为停靠在屏幕角落")
        return nil
    }
    // 屏幕列表需要片刻才会出现新显示器。
    RunLoop.main.run(until: Date().addingTimeInterval(1.0))
    virtualDisplayID = id
    print("隔离窗口使用虚拟显示器 \(id)")
    return id
}

@MainActor
public final class Session {
    public let scenario: String
    public let appPath: String
    public let artifactRoot: URL
    public private(set) var output: URL?
    public private(set) var dataRoot: URL?
    public private(set) var app: Process?
    private(set) var channel: ControlChannel?
    let started = Date()
    var steps: [StepRecord] = []
    var screenshotNames: [String] = []
    var driverEvents: [[String: Any]] = []
    private var logHandle: FileHandle?

    public init(scenario: String, appPath: String, artifactRoot: String) {
        self.scenario = scenario
        self.appPath = appPath
        self.artifactRoot = URL(fileURLWithPath: artifactRoot, isDirectory: true)
    }

    public func begin() throws {
        try FileManager.default.createDirectory(at: artifactRoot, withIntermediateDirectories: true)
        let stamp = ISO8601DateFormatter()
        stamp.formatOptions = [.withInternetDateTime]
        let runID = stamp.string(from: started).replacingOccurrences(of: ":", with: "-")
        let directory = artifactRoot.appendingPathComponent(runID, isDirectory: true)
        let data = directory.appendingPathComponent("data", isDirectory: true)
        try FileManager.default.createDirectory(at: data, withIntermediateDirectories: true)
        output = directory
        dataRoot = data
    }

    public func launch(language: String? = nil) throws {
        guard let dataRoot, let output else {
            throw E2EError.env("运行目录尚未建立")
        }
        if app?.isRunning == true { return }
        let logURL = output.appendingPathComponent("app.log")
        FileManager.default.createFile(atPath: logURL.path, contents: nil)
        let handle = try FileHandle(forWritingTo: logURL)
        let process = Process()
        process.executableURL = URL(fileURLWithPath: appPath)
        var environment = ProcessInfo.processInfo.environment
        environment["NORMA_TEST_DATA_ROOT"] = dataRoot.path
        environment["NORMA_SOCKET"] = isolatedSocketPath(identifier: UUID().uuidString)
        if let display = isolatedDisplayID() {
            environment["NORMA_E2E_DISPLAY_ID"] = String(display)
        }
        let channel = ControlChannel.make()
        environment.merge(channel.environment) { _, new in new }
        process.environment = environment
        // 默认跟随系统偏好；文案写死中文的场景传 language: "zh-Hans"，NORMA_E2E_LANGUAGE 优先于二者。
        if let language = ProcessInfo.processInfo.environment["NORMA_E2E_LANGUAGE"] ?? language {
            process.arguments = ["-AppleLanguages", "(\(language))"]
        }
        process.standardOutput = handle
        process.standardError = handle
        try process.run()
        app = process
        self.channel = channel
        logHandle = handle
        try OwnedProcess.write(pid: process.processIdentifier, executable: appPath, root: artifactRoot)
        print("已启动隔离 Norma，PID \(process.processIdentifier)，数据 \(dataRoot.path)")
    }

    public func stop() {
        guard let process = app else { return }
        if process.isRunning {
            process.terminate()
            let deadline = Date().addingTimeInterval(5)
            while process.isRunning && Date() < deadline {
                Thread.sleep(forTimeInterval: 0.1)
            }
            if process.isRunning {
                kill(process.processIdentifier, SIGKILL)
                process.waitUntilExit()
            }
        }
        try? logHandle?.close()
        logHandle = nil
        if let channel {
            try? FileManager.default.removeItem(atPath: channel.socketPath)
        }
        channel = nil
        OwnedProcess.clear(pid: process.processIdentifier, root: artifactRoot)
        app = nil
    }

    public func ensureRunning() throws {
        guard app?.isRunning == true else {
            throw deathError()
        }
    }

    func deathError() -> E2EError {
        let log = tailLog()
        let conflict = ["AddrInUse", "address already in use", "socket lease", "Address already in use"]
        if conflict.contains(where: { log.localizedCaseInsensitiveContains($0) }) {
            return .env("隔离 Norma 因 socket 或端口冲突退出。未结束用户已有的 Norma。\n\(log)")
        }
        return .product("隔离 Norma 进程已退出，status=\(app?.terminationStatus ?? -1)，reason=\(String(describing: app?.terminationReason))。\n\(log)")
    }
}

enum OwnedProcess {
    static func file(root: URL) -> URL {
        root.appendingPathComponent("owned.json")
    }

    static func write(pid: Int32, executable: String, root: URL) throws {
        let payload: [String: Any] = ["pid": pid, "executable": executable]
        let data = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try data.write(to: file(root: root))
    }

    static func clear(pid: Int32, root: URL) {
        guard let owned = read(root: root), owned.pid == pid else { return }
        try? FileManager.default.removeItem(at: file(root: root))
    }

    static func read(root: URL) -> (pid: Int32, executable: String)? {
        guard let data = try? Data(contentsOf: file(root: root)),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let pid = json["pid"] as? Int,
              let executable = json["executable"] as? String else {
            return nil
        }
        return (Int32(pid), executable)
    }
}
