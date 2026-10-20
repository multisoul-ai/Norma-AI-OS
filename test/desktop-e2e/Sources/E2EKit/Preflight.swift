import AppKit
import CoreGraphics
import Darwin
import Foundation

public enum Preflight {
    public static func check(appPath: String, artifactRoot: URL) throws {
        try FileManager.default.createDirectory(at: artifactRoot, withIntermediateDirectories: true)
        if screenIsLocked() {
            throw E2EError.env("屏幕已锁定。请解锁后重跑；不会发送键鼠，也不会结束用户的 Norma。")
        }
        guard FileManager.default.isExecutableFile(atPath: appPath) else {
            throw E2EError.env("找不到可执行的 Norma：\(appPath)。请先 cargo build -p app。")
        }
        if appPath.contains("/Applications/Norma OS.app") {
            throw E2EError.env("拒绝启动 /Applications/Norma OS.app。桌面 E2E 只运行仓库构建的隔离二进制。")
        }
        try reclaimOwnedProcess(artifactRoot: artifactRoot, expectedExecutable: appPath)
        let foreigners = foreignNormaProcesses()
        if !foreigners.isEmpty {
            let lines = foreigners.map { "PID \($0.pid) \($0.path)" }.joined(separator: "；")
            print("检测到其他 Norma 进程，不会结束它们：\(lines)")
            print("本次使用独立 NORMA_TEST_DATA_ROOT 与 NORMA_SOCKET。若启动后立刻退出，将记为 ENV。")
        }
    }

    private static func screenIsLocked() -> Bool {
        guard let raw = CGSessionCopyCurrentDictionary() else { return false }
        let session = raw as NSDictionary
        if let locked = session["CGSSessionScreenIsLocked"] as? Bool { return locked }
        if let number = session["CGSSessionScreenIsLocked"] as? NSNumber { return number.boolValue }
        return false
    }

    /// 只结束本脚手架上次留下、且可执行文件路径与本次测试二进制一致的 PID。
    private static func reclaimOwnedProcess(artifactRoot: URL, expectedExecutable: String) throws {
        guard let owned = OwnedProcess.read(root: artifactRoot) else { return }
        let path = executablePath(pid: owned.pid)
        guard path != nil else {
            OwnedProcess.clear(pid: owned.pid, root: artifactRoot)
            return
        }
        if path?.contains("/Applications/Norma OS.app") == true {
            throw E2EError.env("owned.json 指向用户的 /Applications/Norma OS.app（PID \(owned.pid)）。已停止，未结束该进程。")
        }
        guard path == expectedExecutable || path == owned.executable else {
            print("忽略 owned.json 中的 PID \(owned.pid)，其路径不是本次测试二进制：\(path ?? "")")
            return
        }
        print("结束上次桌面 E2E 留下的隔离进程 PID \(owned.pid)")
        kill(owned.pid, SIGTERM)
        let deadline = Date().addingTimeInterval(3)
        while Date() < deadline && kill(owned.pid, 0) == 0 {
            Thread.sleep(forTimeInterval: 0.1)
        }
        if kill(owned.pid, 0) == 0 { kill(owned.pid, SIGKILL) }
        OwnedProcess.clear(pid: owned.pid, root: artifactRoot)
    }

    private static func foreignNormaProcesses() -> [(pid: Int32, path: String)] {
        NSWorkspace.shared.runningApplications.compactMap { application in
            let path = application.executableURL?.path ?? ""
            let name = application.localizedName ?? ""
            let bundle = application.bundleIdentifier ?? ""
            let looksLikeNorma = bundle == "com.norma.os" || name == "Norma OS" || path.hasSuffix("/norma-os")
            guard looksLikeNorma else { return nil }
            return (application.processIdentifier, path.isEmpty ? name : path)
        }
    }

    private static func executablePath(pid: Int32) -> String? {
        var buffer = [CChar](repeating: 0, count: Int(MAXPATHLEN))
        let length = proc_pidpath(pid, &buffer, UInt32(buffer.count))
        guard length > 0 else { return nil }
        return String(cString: buffer)
    }
}
