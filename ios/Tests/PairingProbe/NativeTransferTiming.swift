#if DEBUG
import Foundation
@testable import NormaCore

/// Test-only wire measurement reuses the shipping pin/timeout and fragment decoder.
/// No observer, logging switch or timing API is added to the phone application.
enum NativeTransferTiming {
    /// 场景：系统取消订阅后，双地址并行探路必须及时复用服务容量。
    /// 数据构造：隔离Keychain手机，不登记App电脑列表；8条连接为正式上限，2条真实路径和1条TLS停滞路径。
    ///   24次替换=3轮完整8槽位；250ms=50ms合并+20ms监听+180ms原生TLS调度预算，小于2s心跳。
    /// 执行过程：正式授权 → 开满8条计划订阅 → 每次系统取消一条后立即重建，共24次。
    /// 预期结果：每次合法基线且无错误/重试；只读取空计划，不执行Agent、不混入App UI自己的订阅。
    @MainActor static func watchLifecycle(client: MobileClient, machine: PairedMachine) async throws {
        let grant = try await client.authenticate(machine)
        guard machine.endpoints.count >= 2, let workspace = grant.projects.first?.workspaceID,
              let credential = try await client.vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        func open() async throws -> PinnedConnection {
            let connection = try await PinnedConnection.open(machine, streaming: true)
            do {
                let id = UUID().uuidString
                let body = try JSONSerialization.data(withJSONObject: ["version":1,"request_id":id,"machine_id":machine.id,
                    "action":["type":"watch_plan","device_id":credential.deviceID,"token":credential.token,"workspace_id":workspace]])
                let data = try await connection.exchange(body)
                let reply = try WireReply.decode(data, expectedID: id)
                guard reply.result?.status == "plan_event", reply.result?.planEvent?.snapshot != nil else { throw MobileError.invalidResponse }
                return connection
            } catch { connection.close(); throw error }
        }
        var connections: [PinnedConnection] = []
        defer { connections.forEach { $0.close() } }
        for index in 0..<8 {
            print("NATIVE_WATCH_OPEN_INITIAL=\(index)")
            connections.append(try await open())
        }
        var timings: [Duration] = []
        for index in 0..<24 {
            print("NATIVE_WATCH_REPLACE=\(index)")
            let began = ContinuousClock.now
            connections.removeFirst().close()
            connections.append(try await open())
            let elapsed = began.duration(to: .now)
            timings.append(elapsed)
            guard elapsed < .milliseconds(250) else { throw MobileError.timedOut }
        }
        try NativeTiming.report("watch_replacement", provider: "isolated WSS", samples: timings,
            scope: "system cancellation + parallel pinned routes + new plan snapshot; excludes SwiftUI/physical LAN/Tailscale")
        print("NATIVE_WATCH_LIFECYCLE_OK")
    }

    /// 数据：已逐字校验的大页，1s稳定期，服务owner独立采样三个2s心跳周期。
    /// 过程：独立Task持续消费公共订阅 → ready → libproc/nettop → done → 检查至少两个后续事件。
    /// 预期：无恢复/终止，完整页面保留；不把暂停消费、客户端CPU或JSON字节当服务端证据。
    @MainActor static func idle(client: MobileClient, machine: PairedMachine, request: ConversationRequest,
                               expected: ConversationResponse, directory: URL) async throws {
        let watch = ConversationWatchCheck(client: client, machine: machine, request: request)
        defer { watch.cancel() }
        try await watch.require(expected.page.items[0].text)
        try await Task.sleep(for: .seconds(1))
        try watch.requireActive()
        let received = watch.received, contentUpdates = watch.contentUpdates
        let cpu = try NativeTiming.cpuSeconds(), started = ContinuousClock.now
        try Data().write(to: directory.appendingPathComponent("native-idle-ready"), options: .atomic)
        //15s =6s sampling +5s nettop startup bound +4s test coordination margin.
        let deadline = started.advanced(by: .seconds(15))
        while !FileManager.default.fileExists(atPath: directory.appendingPathComponent("native-idle-done").path) {
            try watch.requireActive()
            guard ContinuousClock.now < deadline else { throw MobileError.timedOut }
            try await Task.sleep(for: .milliseconds(100))
        }
        let elapsed = NativeTiming.milliseconds(ContinuousClock.now - started) / 1000
        let consumed = try NativeTiming.cpuSeconds() - cpu
        try watch.requireActive()
        guard watch.received - received >= 2, watch.contentUpdates == contentUpdates,
              watch.timeline.items == expected.page.items else { throw MobileError.invalidResponse }
        let result: [String: Any] = ["wall_seconds": elapsed, "client_cpu_seconds": consumed,
            "client_one_core_percent": consumed / elapsed * 100, "received_updates": watch.received - received,
            "content_updates": watch.contentUpdates - contentUpdates,
            "scope": "native probe/test host actively consuming public conversationUpdates; includes fixture timeline merges and100ms checkpoint polling; excludes server, Provider and complete SwiftUI"]
        print("NATIVE_IDLE_CHECKPOINT=" + String(decoding: try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys]), as: UTF8.self))
        await watch.finish()
        print("NATIVE_IDLE_CHECKPOINT_OK")
    }

    static func run(client: MobileClient, machine: PairedMachine, request: ConversationRequest, expected: ConversationResponse) async throws {
        var prepare: [Duration] = [], connect: [Duration] = [], first: [Duration] = [], complete: [Duration] = []
        var counts: [[String: Int]] = []
        //20 independent connections: nearest-rank p50 is observation10, p95 is observation19.
        // Each read retains the full fixture check; this does not generate extra model turns.
        for _ in 0..<20 {
            let started = ContinuousClock.now
            guard let credential = try await client.vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
            let id = UUID().uuidString
            let body = try JSONSerialization.data(withJSONObject: ["version":1,"request_id":id,"machine_id":machine.id,
                "action":["type":"conversation","device_id":credential.deviceID,"token":credential.token,
                    "request":try JSONSerialization.jsonObject(with: JSONEncoder().encode(request))]])
            let prepared = ContinuousClock.now
            let connection = try await PinnedConnection.open(machine, streaming: true)
            defer { connection.close() }
            let connected = ContinuousClock.now
            var data = try await connection.exchange(body)
            let firstFrame = ContinuousClock.now
            var transfer = ConversationTransfer()
            var frames = 0, bytes = 0
            let response: ConversationResponse
            while true {
                frames += 1; bytes += data.count
                let reply = try WireReply.decode(data, expectedID: id)
                guard reply.result?.status == "conversation_fragment", let fragment = reply.result?.fragment else { throw MobileError.invalidResponse }
                if let page = try transfer.append(fragment, as: ConversationResponse.self) { response = page; break }
                data = try await connection.receive()
            }
            try response.validate(for: request)
            guard frames > 1, response.page.items == expected.page.items, response.page.cursor == expected.page.cursor,
                  response.page.hasMore == expected.page.hasMore, response.page.pendingTail == expected.page.pendingTail else { throw MobileError.invalidResponse }
            prepare.append(prepared - started); connect.append(connected - prepared)
            first.append(firstFrame - connected); complete.append(ContinuousClock.now - connected)
            counts.append(["frames":frames,"response_json_bytes":bytes,"request_json_bytes":body.count])
        }
        try NativeTiming.report("conversation_keychain_and_prepare", provider: "Codex fixture", samples: prepare,
            scope: "native isolated Keychain credential read and request encoding; no connection or model")
        try NativeTiming.report("conversation_tls_ws_ready", provider: "Codex fixture", samples: connect,
            scope: "shipping PinnedConnection TLS1.3/pin + WebSocket + readiness Ping/Pong; not pure TLS time")
        try NativeTiming.report("conversation_first_fragment", provider: "Codex fixture", samples: first,
            scope: "native send to first complete PinnedConnection frame Data, before WireReply decode; frozen large fixture, not Provider first token or raw TCP first byte")
        try NativeTiming.report("conversation_full_page_verified", provider: "Codex fixture", samples: complete,
            scope: "native send + all fragments + shared decoder + exact full fixture validation; excludes preceding Keychain/connection time")
        print("NATIVE_TRANSFER_BYTES=" + String(decoding: try JSONSerialization.data(withJSONObject:
            ["samples":counts,"scope":"UTF-8 application JSON; excludes TLS/TCP headers and connection handshake"], options: [.sortedKeys]), as: UTF8.self))
        print("NATIVE_FIRST_FRAGMENT_TIMING_OK")
    }
}
#endif
