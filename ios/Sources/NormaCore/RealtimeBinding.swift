import Foundation

extension MachineSnapshot {
    public func controlProject(_ workspaceID: String) throws -> ProjectSnapshot {
        guard !grant.revoked,
              grant.projects.contains(where: { $0.id == workspaceID && $0.access == "control" }),
              let project = projects.first(where: { $0.id == workspaceID }) else { throw MobileError.unauthorized }
        return project
    }
}

/// The phone owns only avatar tools. Mac failure must not leave unavailable task tools
/// advertised to the voice model or make the phone synthesize an execution receipt.
public struct RealtimeLocalConfiguration {
    private var initial: [String: Any]?
    public init() {}
    public mutating func capture(_ session: [String: Any]) {
        initial = ["type":"realtime", "instructions":session["instructions"] as? String ?? "",
            "tools":(session["tools"] as? [[String: Any]] ?? []).filter { $0["name"] as? String == "set_behavior" }]
    }
    public var offlineUpdate: [String: Any]? {
        guard var session = initial else { return nil }
        session["instructions"] = (session["instructions"] as? String ?? "") + "\n当前 Mac 任务工具不可用；可以继续讨论和整理想法，不能声称任务已接收或已完成。已提交但未核对的操作保持未知，不要重新派工。"
        return ["type":"session.update", "session":session]
    }
}

public struct RealtimeBinding: Codable, Sendable, Equatable {
    public let callID: String
    public let workspaceID: String
    public let generation: UInt64
    enum CodingKeys: String, CodingKey { case callID = "call_id", workspaceID = "workspace_id", generation }
    static func validCallID(_ callID: String) -> Bool {
        validID(callID) && ((callID.hasPrefix("rtc_") && callID.count > 4)
            || (callID.hasPrefix("doubao_") && callID.count > 7))
    }
    public init(callID: String, workspaceID: String, generation: UInt64) throws {
        guard Self.validCallID(callID), validID(workspaceID), generation > 0 else {
            throw MobileError.invalidResponse
        }
        self.callID = callID; self.workspaceID = workspaceID; self.generation = generation
    }
}

/// One-use gateway capability. Encoding deliberately omits the separately validated media ID.
public struct RealtimeGatewayTicket: Encodable, Sendable {
    public let callID: String
    public let url: URL
    public let token: String
    public let expiresAt: TimeInterval
    enum CodingKeys: String, CodingKey { case url, token, expiresAt = "expires_at" }

    public init(data: Data, endpoint: URL, callID: String, now: TimeInterval = Date().timeIntervalSince1970) throws {
        guard data.count <= 8192,
              let event = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              callID.hasPrefix("doubao_"), callID.count > 7, validID(callID), event["call_id"] as? String == callID,
              let address = event["url"] as? String, let url = URL(string: address),
              VoiceSessionTicket.matchesGateway(url, endpoint: endpoint, path: "/v1/tools"),
              let token = event["token"] as? String, token.utf8.count == 43,
              token.utf8.allSatisfy({ (65...90).contains($0) || (97...122).contains($0) || (48...57).contains($0) || $0 == 45 || $0 == 95 }),
              let expires = event["expires_at"] as? Double, expires > now, expires <= now + 60 else { throw MobileError.invalidResponse }
        self.callID = callID; self.url = url; self.token = token; self.expiresAt = expires
    }
}

public struct RealtimeBindingStatus: Decodable, Sendable {
    public enum State: String, Decodable, Sendable { case connecting, attached, ready, unavailable }
    public let state: State
    public let error: String?
    public let binding: RealtimeBinding
    public let grant: DeviceGrant
    private let snapshotObservedAtUnixMS: UInt64?
    public var snapshotObservedAt: Date? {
        snapshotObservedAtUnixMS.map { Date(timeIntervalSince1970: Double($0) / 1000) }
    }
    let status: String
    let machineID: String
    public var toolsAvailable: Bool { state == .ready }
    enum CodingKeys: String, CodingKey {
        case state, error, binding, grant, status, machineID = "machine_id"
        case snapshotObservedAtUnixMS = "snapshot_observed_at_unix_ms"
    }

    static func decode(_ data: Data, requestID: String, machineID: String, deviceID: String, binding: RealtimeBinding) throws -> Self {
        _ = try WireReply.decode(data, expectedID: requestID)
        struct Envelope: Decodable { let result: RealtimeBindingStatus }
        let value = try JSONDecoder().decode(Envelope.self, from: data).result
        try value.grant.validate(deviceID: deviceID)
        if let milliseconds = value.snapshotObservedAtUnixMS {
            guard milliseconds > 0, Double(milliseconds) / 1000 <= Date.distantFuture.timeIntervalSince1970 else {
                throw MobileError.invalidResponse
            }
        }
        guard value.status == "realtime_binding", value.machineID == machineID, value.binding == binding,
              value.grant.projects.contains(where: { $0.id == binding.workspaceID && $0.access == "control" }),
              (value.state == .unavailable) == (value.error != nil) else { throw MobileError.invalidResponse }
        return value
    }
}

extension MobileClient {
    public func prepareRealtimeBinding(_ machine: PairedMachine) async throws -> RealtimeBindingClient {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return RealtimeBindingClient(machine: machine, credential: credential)
    }
}

/// Retains only this call's paired credential in memory so an already-started call can reconnect
/// while locked. Keychain accessibility is unchanged; the call releases this value on hangup.
public struct RealtimeBindingClient: Sendable {
    let machine: PairedMachine
    let credential: DeviceCredential
    /// A dedicated pinned connection owned by the call. Closing this stream unbinds tools only;
    /// it never sends task cancellation or replays an execution command.
    public func updates(binding: RealtimeBinding, gateway: RealtimeGatewayTicket? = nil) -> AsyncThrowingStream<RealtimeBindingStatus, any Error> {
        AsyncThrowingStream(bufferingPolicy: .bufferingNewest(1)) { continuation in
            let work = Task {
                do {
                    try machine.validate()
                    guard binding.callID.hasPrefix("doubao_") == (gateway != nil),
                          gateway == nil || (gateway?.callID == binding.callID && (gateway?.expiresAt ?? 0) > Date().timeIntervalSince1970) else { throw MobileError.invalidResponse }
                    let connection = try await PinnedConnection.open(machine, streaming: true)
                    defer { connection.close() }
                    let id = UUID().uuidString
                    let bindingObject = try JSONSerialization.jsonObject(with: JSONEncoder().encode(binding))
                    var action: [String: Any] = ["type":"bind_realtime","device_id":credential.deviceID,"token":credential.token,"binding":bindingObject]
                    if let gateway { action["gateway"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(gateway)) }
                    let body = try JSONSerialization.data(withJSONObject: ["version":1,"request_id":id,"machine_id":machine.id,"action":action])
                    var data = try await connection.exchange(body)
                    var revision: UInt64?
                    while !Task.isCancelled {
                        let status = try RealtimeBindingStatus.decode(data, requestID: id, machineID: machine.id,
                            deviceID: credential.deviceID, binding: binding)
                        if let revision, revision != status.grant.revision { throw MobileError.grantChanged }
                        revision = status.grant.revision
                        if case .terminated = continuation.yield(status) { throw CancellationError() }
                        if status.state == .unavailable { break }
                        data = try await connection.receive()
                    }
                    continuation.finish()
                } catch { continuation.finish(throwing: error) }
            }
            continuation.onTermination = { _ in work.cancel() }
        }
    }
}
