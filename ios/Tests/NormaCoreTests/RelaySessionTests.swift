import Foundation
import XCTest
@testable import NormaCore

final class RelaySessionTests: XCTestCase {
    /// 原生密码库与独立 Node 对端互通，并固定配对证书、通道、方向和消息序号。
    /// 数据构造：1 张临时 Mac 证书；每方向序号从 0 开始；帧开销 = 8 字节序号 + 16 字节标签。
    /// 执行过程：启动本机协议对端 → 手机产生 hello → 验证 Mac 握手 → 解密回复 → 加密任务 → 对端解密。
    ///   另测错误 pin、重放与握手重复消费；超过上限 1 字节的输入必须失败。
    /// 预期结果：双向中文原样恢复；错误身份与重复帧拒绝，不暴露明文、不复用发送 nonce。
    func testNativeSessionInteroperabilityAndIdentityChecks() async throws {
        #if os(macOS)
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let process = Process(), input = Pipe(), output = Pipe()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["node", try XCTUnwrap(Bundle.module.url(forResource: "relay-peer", withExtension: "cjs")).path, directory.path]
        process.standardInput = input; process.standardOutput = output
        try process.run()
        defer { if process.isRunning { process.terminate() }; input.fileHandleForWriting.closeFile() }
        let identity = try jsonLine(output.fileHandleForReading)
        let machineID = try XCTUnwrap(identity["machine_id"] as? String)
        let stream = "00000000-0000-4000-8000-000000000001"
        let client = try RelaySessionClient(streamID: stream, machineID: machineID)
        let hello = try JSONSerialization.jsonObject(with: JSONEncoder().encode(client.hello))
        try sendLine(["stream": stream, "hello": hello], input.fileHandleForWriting)
        let reply = try jsonLine(output.fileHandleForReading)
        let handshake = try JSONDecoder().decode(RelayHandshake.self, from: JSONSerialization.data(withJSONObject: try XCTUnwrap(reply["handshake"])))
        let cipher = try await client.finish(handshake)
        do { _ = try await client.finish(handshake); XCTFail("同一握手不能产出两个复用 nonce 的 session") }
        catch { XCTAssertTrue(error is RelaySessionError, "重复握手应明确失败") }
        let encrypted = try XCTUnwrap(Data(base64Encoded: try XCTUnwrap(reply["frame"] as? String)))
        XCTAssertFalse(String(decoding: encrypted, as: UTF8.self).contains("Mac 已收到"), "中继不得读取明文")
        var corrupted = encrypted; corrupted[corrupted.count - 1] ^= 1
        do { _ = try await cipher.open(corrupted); XCTFail("篡改标签必须在交付明文前拒绝") }
        catch { XCTAssertTrue(error is RelaySessionError, "认证失败应为明确连接错误") }
        let plaintext = try await cipher.open(encrypted)
        XCTAssertEqual(String(decoding: plaintext, as: UTF8.self), "Mac 已收到", "手机必须验证并完整解密 Mac 消息")
        do { _ = try await cipher.open(encrypted); XCTFail("原密文不能重放") }
        catch { XCTAssertTrue(error is RelaySessionError, "重放必须报序号错误") }
        // 256 KiB frame - 8-byte sequence - 16-byte tag + 1 = first rejected plaintext size.
        do { _ = try await cipher.seal(Data(count: 256 * 1024 - 23)); XCTFail("超过帧预算不能静默截断") }
        catch { XCTAssertTrue(error is RelaySessionError, "超限应在加密前拒绝") }
        let outbound = try await cipher.seal(Data("修改首屏".utf8))
        XCTAssertEqual(Array(outbound.prefix(8)), Array(repeating: 0, count: 8), "超限失败不能消耗或破坏首个合法序号")
        do { _ = try await cipher.open(outbound); XCTFail("上行密文不能反射成为 Mac 回复") }
        catch { XCTAssertTrue(error is RelaySessionError, "方向或序号错误必须拒绝") }
        try sendLine(["frame": outbound.base64EncodedString()], input.fileHandleForWriting)
        XCTAssertEqual(try jsonLine(output.fileHandleForReading)["plaintext"] as? String, "修改首屏", "独立对端必须解密手机的真实 AES-GCM 输出")
        let wrong = try RelaySessionClient(streamID: stream, machineID: "mac-" + String(repeating: "0", count: 64))
        do { _ = try await wrong.finish(handshake); XCTFail("不匹配的配对证书不能信任") }
        catch { XCTAssertTrue(error is RelaySessionError, "错误 pin 必须明确失败") }
        input.fileHandleForWriting.closeFile(); process.waitUntilExit()
        XCTAssertEqual(process.terminationStatus, 0, "独立协议对端应正常完成")
        #else
        throw XCTSkip("Node 跨语言检查在 macOS Core 测试运行；真机不运行进程夹具。")
        #endif
    }

    /// 握手在生成密钥前校验通道与预期 Mac 身份。
    /// 数据构造：1 个非法通道和 1 个缺少 64 hex 指纹的 Mac ID；均不需要网络或 Keychain。
    /// 执行过程：从公开入口建立客户端。
    /// 预期结果：两个非法输入均失败，不创建可发送配对凭据的会话。
    func testInvalidRelayIdentityCannotStart() throws {
        XCTAssertThrowsError(try RelaySessionClient(streamID: "other\nstream", machineID: "mac-" + String(repeating: "a", count: 64)), "非 UUID 通道不得进入签名转录")
        XCTAssertThrowsError(try RelaySessionClient(streamID: UUID().uuidString, machineID: "mac-invalid"), "必须指定已经配对的完整证书指纹")
    }

    #if os(macOS)
    private func jsonLine(_ handle: FileHandle) throws -> [String: Any] {
        var line = Data()
        while let data = try handle.read(upToCount: 1), !data.isEmpty {
            if data == Data([10]) { break }
            line.append(data)
        }
        return try XCTUnwrap(JSONSerialization.jsonObject(with: line) as? [String: Any])
    }
    private func sendLine(_ value: [String: Any], _ handle: FileHandle) throws {
        var bytes = try JSONSerialization.data(withJSONObject: value); bytes.append(10)
        try handle.write(contentsOf: bytes)
    }
    #endif
}
