import Darwin
import E2EKit
import Foundation

/// 本机回环上的假方舟服务：`/chat` 返回固定的 stop 回复并计数，`/tts` 返回一小段静音 PCM。
/// 用来证明“未命中名字时请求确实回落到模型”以及“命中名字时模型一次都没被问到”。
public final class FakeArkServer: @unchecked Sendable {
    private let lock = NSLock()
    private var chats = 0
    private let listener: Int32
    public let port: UInt16

    public var chatURL: String { "http://127.0.0.1:\(port)/chat" }
    public var ttsURL: String { "http://127.0.0.1:\(port)/tts" }
    public var chatRequests: Int { lock.lock(); defer { lock.unlock() }; return chats }

    public init() throws {
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else { throw E2EError.env("无法创建假方舟 socket") }
        var yes: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &yes, socklen_t(MemoryLayout<Int32>.size))
        var address = sockaddr_in()
        address.sin_family = sa_family_t(AF_INET)
        address.sin_addr.s_addr = inet_addr("127.0.0.1")
        address.sin_port = 0
        let bound = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size)) }
        }
        guard bound == 0, listen(fd, 16) == 0 else {
            close(fd)
            throw E2EError.env("假方舟无法监听回环端口")
        }
        var assigned = sockaddr_in()
        var length = socklen_t(MemoryLayout<sockaddr_in>.size)
        _ = withUnsafeMutablePointer(to: &assigned) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { getsockname(fd, $0, &length) }
        }
        listener = fd
        port = UInt16(bigEndian: assigned.sin_port)
        Thread.detachNewThread { [self] in acceptLoop() }
    }

    public func stop() { close(listener) }

    private func acceptLoop() {
        while true {
            let client = accept(listener, nil, nil)
            if client < 0 { return }
            Thread.detachNewThread { [self] in serve(client) }
        }
    }

    private func serve(_ client: Int32) {
        defer { close(client) }
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 8192)
        var headerEnd: Range<Data.Index>?
        while headerEnd == nil {
            let count = read(client, &buffer, buffer.count)
            if count <= 0 { return }
            data.append(contentsOf: buffer[0..<count])
            headerEnd = data.range(of: Data("\r\n\r\n".utf8))
        }
        guard let headerEnd, let head = String(data: data[..<headerEnd.lowerBound], encoding: .utf8) else { return }
        let path = head.split(separator: " ").dropFirst().first.map(String.init) ?? ""
        let length = head.split(separator: "\r\n")
            .first { $0.lowercased().hasPrefix("content-length:") }
            .flatMap { Int($0.dropFirst("content-length:".count).trimmingCharacters(in: .whitespaces)) } ?? 0
        while data.count - headerEnd.upperBound < length {
            let count = read(client, &buffer, buffer.count)
            if count <= 0 { return }
            data.append(contentsOf: buffer[0..<count])
        }
        let body: String
        if path == "/chat" {
            lock.lock(); chats += 1; lock.unlock()
            body = #"{"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"好的，我来处理。"}}]}"#
        } else {
            let silence = Data(count: 9600).base64EncodedString()
            body = "{\"code\":0,\"data\":\"\(silence)\"}\n{\"code\":20000000}\n"
        }
        let payload = Data(body.utf8)
        let header = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: \(payload.count)\r\nConnection: close\r\n\r\n"
        let response = Data(header.utf8) + payload
        response.withUnsafeBytes { raw in
            var offset = 0
            while offset < raw.count {
                let written = write(client, raw.baseAddress! + offset, raw.count - offset)
                if written <= 0 { return }
                offset += written
            }
        }
    }
}
