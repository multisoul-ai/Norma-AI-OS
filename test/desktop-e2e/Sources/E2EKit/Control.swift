import CoreGraphics
import Darwin
import Foundation

/// debug 构建 Norma 的测试控制通道：点击、按键和截图都由 App 在自己的窗口内完成，
/// 不经过系统级 CGEvent / 屏幕录制，因此不需要辅助功能或屏幕录制授权。
struct ControlChannel {
    let socketPath: String
    let token: String

    static func make() -> ControlChannel {
        let id = UUID().uuidString.lowercased().filter { $0 != "-" }
        let token = (UUID().uuidString + UUID().uuidString).lowercased().filter { $0 != "-" }
        return ControlChannel(socketPath: "/tmp/norma-e2e-ctl-\(id.prefix(24)).sock", token: token)
    }

    var environment: [String: String] {
        ["NORMA_E2E_CONTROL_SOCKET": socketPath, "NORMA_E2E_CONTROL_TOKEN": token]
    }

    /// 发送一条命令并读取一行 JSON 响应。连不上返回 nil，由调用方决定等待还是判定死亡。
    func send(_ payload: [String: Any]) throws -> [String: Any]? {
        var request = payload
        request["token"] = token
        let body = try JSONSerialization.data(withJSONObject: request) + Data([0x0A])

        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw E2EError.env("无法创建控制通道 socket") }
        defer { close(fd) }
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = Array(socketPath.utf8)
        guard pathBytes.count < MemoryLayout.size(ofValue: address.sun_path) else {
            throw E2EError.env("控制通道路径过长：\(socketPath)")
        }
        withUnsafeMutableBytes(of: &address.sun_path) { raw in
            raw.copyBytes(from: pathBytes)
        }
        let connected = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connected == 0 else { return nil }

        try body.withUnsafeBytes { raw in
            var offset = 0
            while offset < raw.count {
                let written = write(fd, raw.baseAddress! + offset, raw.count - offset)
                guard written > 0 else { throw E2EError.env("控制通道写入失败") }
                offset += written
            }
        }
        var response = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)
        while !response.contains(0x0A) {
            let count = read(fd, &buffer, buffer.count)
            guard count > 0 else { break }
            response.append(contentsOf: buffer[0..<count])
        }
        guard let line = response.split(separator: 0x0A).first,
              let json = try JSONSerialization.jsonObject(with: Data(line)) as? [String: Any] else {
            throw E2EError.env("控制通道响应无法解析")
        }
        return json
    }
}

struct ControlWindow {
    let id: CGWindowID
    let frame: CGRect
}

extension Session {
    /// 等 App 启动完成并开放控制通道；App 中途退出时按死亡处理。
    @discardableResult
    func control(_ payload: [String: Any]) async throws -> [String: Any] {
        guard let channel else { throw E2EError.env("控制通道尚未建立") }
        let deadline = Date().addingTimeInterval(20)
        while true {
            try ensureRunning()
            if let response = try channel.send(payload) {
                guard response["ok"] as? Bool == true else {
                    let reason = response["error"] as? String ?? "unknown"
                    throw E2EError.product("App 控制通道拒绝 \(payload["op"] ?? "")：\(reason)")
                }
                return response
            }
            if Date() >= deadline {
                throw E2EError.env("20 秒内连不上 App 控制通道 \(channel.socketPath)。请确认使用 debug 构建（cargo build -p app，不带 production-data）。")
            }
            try await Task.sleep(nanoseconds: 150_000_000)
        }
    }

    /// 运行特效验收：让 App 切到画布并摆出六张不同运行状态的 Agent 卡片。
    public func installRunEffectsFixture() async throws {
        try await control(["op": "run_effects_fixture"])
    }

    func controlWindow(_ response: [String: Any]) throws -> ControlWindow {
        guard let window = response["window"] as? [String: Any],
              let number = window["number"] as? NSNumber,
              let width = window["width"] as? NSNumber,
              let height = window["height"] as? NSNumber else {
            throw E2EError.env("控制通道没有返回窗口信息")
        }
        return ControlWindow(
            id: CGWindowID(number.uint32Value),
            frame: CGRect(x: 0, y: 0, width: width.doubleValue, height: height.doubleValue)
        )
    }
}
