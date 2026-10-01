import Foundation

/// Cloud admission routes an opaque session; only the paired Mac can authenticate project access.
public struct CollaborationRelayClient: Sendable {
    private let origin: URL
    private let certificatePin: String?
    public init() { origin = CloudTextClient.serviceOrigin; certificatePin = nil }

    #if DEBUG
    public init(localPort: UInt16) throws {
        guard localPort > 0 else { throw MobileError.invalidOffer }
        origin = URL(string: "http://127.0.0.1:\(localPort)/")!
        certificatePin = nil
    }
    #endif
    public init(local: LocalCloudConfiguration) {
        origin = local.origin; certificatePin = local.certificateSHA256
    }

    public func connect(_ machine: PairedMachine, cloud: CloudDeviceCredential,
                        vault: CredentialVault) async throws -> CollaborationRelayConnection {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try await connect(machineID: machine.id, cloud: cloud, credential: credential)
    }

    func connect(machineID: String, cloud: CloudDeviceCredential,
                 credential: DeviceCredential) async throws -> CollaborationRelayConnection {
        guard machineID.hasPrefix("mac-"), validHex(String(machineID.dropFirst(4))),
              UUID(uuidString: cloud.deviceID) != nil,
              cloud.token.range(of: "^[A-Za-z0-9_-]{43}$", options: .regularExpression) != nil,
              validID(credential.deviceID), validHex(credential.token) else { throw MobileError.invalidCredential }
        try Task.checkCancellation()
        // Open + signed handshake + paired authentication are reserved together, before a socket
        // starts its 10s handshake deadline. All connections to this Mac share the same window.
        try await RelayRequestBudget.shared.reserve(machineID: machineID, frames: 3)
        var components = URLComponents(url: origin.appending(path: "v1/relay/phone"), resolvingAgainstBaseURL: false)!
        components.scheme = origin.scheme == "https" ? "wss" : "ws"
        var request = URLRequest(url: components.url!, timeoutInterval: 10)
        request.setValue("Bearer \(cloud.token)", forHTTPHeaderField: "Authorization")
        let config = URLSessionConfiguration.ephemeral
        config.tlsMinimumSupportedProtocolVersion = .TLSv13
        config.urlCache = nil; config.httpCookieStorage = nil; config.urlCredentialStorage = nil
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        let delegate: any URLSessionDelegate = certificatePin.map { PinDelegate(pin: $0) } ?? CloudNoRedirects()
        let session = URLSession(configuration: config, delegate: delegate, delegateQueue: nil)
        let socket = session.webSocketTask(with: request)
        // Same budget as Node: base64(256 KiB) + 1 KiB metadata.
        socket.maximumMessageSize = 4 * ((256 * 1024 + 2) / 3) + 1024
        let transport = PinnedConnection(session: session, socket: socket)
        socket.resume()
        do {
            // 10s bounds the entire open + signed handshake + paired authorization, including DNS/TLS.
            return try await transport.bounded(timeout: .seconds(10)) {
                let id = UUID().uuidString
                let request = try JSONSerialization.data(withJSONObject: ["type": "open", "request_id": id, "machine_id": machineID])
                let opened = try JSONDecoder().decode(RelayEnvelope.self, from: await transport.exchange(request))
                guard opened.requestID == id else { throw MobileError.invalidResponse }
                if opened.type == "offline", opened.machineID == machineID { throw MobileError.connectionFailed }
                if opened.type == "error", opened.error == "busy" { throw MobileError.busy }
                guard opened.type == "open", opened.machineID == machineID, let stream = opened.streamID,
                      UUID(uuidString: stream) != nil else { throw MobileError.invalidResponse }
                let offer = try RelaySessionClient(streamID: stream, machineID: machineID)
                let data = try await transport.exchange(relayPayload(try JSONEncoder().encode(offer.hello), stream: stream))
                let handshake = try JSONDecoder().decode(RelayHandshake.self, from: relayBytes(data, stream: stream))
                let cipher = try await offer.finish(handshake)
                let connection = CollaborationRelayConnection(transport: transport, cipher: cipher,
                    stream: stream, machineID: machineID, credential: credential)
                try await connection.authenticate()
                try Task.checkCancellation()
                return connection
            }
        } catch {
            transport.close()
            try Task.checkCancellation()
            throw relayConnectionError(error)
        }
    }
}

public actor CollaborationRelayConnection {
    public private(set) var capabilities: [String] = []
    public private(set) var grant: DeviceGrant?
    private let transport: PinnedConnection
    private let stream: String
    public nonisolated let machineID: String
    private let credential: DeviceCredential
    private var cipher: RelayEncryptedSession?
    private var inFlight = false

    fileprivate init(transport: PinnedConnection, cipher: RelayEncryptedSession, stream: String,
                     machineID: String, credential: DeviceCredential) {
        self.transport = transport; self.cipher = cipher; self.stream = stream
        self.machineID = machineID; self.credential = credential
    }
    deinit { transport.close() }

    fileprivate func authenticate() async throws {
        let reply = try await exchange(type: "authenticate")
        guard reply.result?.status == "authorized", let grant = reply.result?.grant else { throw MobileError.invalidResponse }
        try grant.validate(deviceID: credential.deviceID)
        let advertised = reply.result?.capabilities ?? []
        guard advertised.count <= 16, advertised.allSatisfy({ validID($0) }) else { throw MobileError.invalidResponse }
        self.grant = grant; capabilities = advertised
    }

    public func snapshot(workspaceID: String? = nil) async throws -> MachineSnapshot {
        guard let grant, !grant.revoked else { throw MobileError.unauthorized }
        if let workspaceID, !validID(workspaceID) { throw MobileError.invalidResponse }
        let reply = try await exchange(type: "snapshot", workspaceID: workspaceID)
        do { return try MachineSnapshot(reply: reply, deviceID: credential.deviceID, workspaceID: workspaceID) }
        catch { close(); throw error }
    }

    public func close() { cipher = nil; transport.close() }

    func exchange(type: String, workspaceID: String? = nil, fields: [String: Any] = [:]) async throws -> WireReply {
        guard let cipher else { throw MobileError.connectionFailed }
        // Actor reentrancy alone does not serialize an exchange across await points.
        guard !inFlight else { throw MobileError.busy }
        inFlight = true
        defer { inFlight = false }
        do {
            try Task.checkCancellation()
            if type != "authenticate" {
                try await RelayRequestBudget.shared.reserve(machineID: machineID, frames: 1)
            }
            let id = UUID().uuidString
            var action: [String: Any] = fields
            action["type"] = type; action["device_id"] = credential.deviceID; action["token"] = credential.token
            if let workspaceID { action["workspace_id"] = workspaceID }
            let request = try JSONSerialization.data(withJSONObject: ["version": 1, "request_id": id,
                "machine_id": machineID, "action": action])
            let sealed = try await cipher.seal(request)
            guard self.cipher != nil else { throw MobileError.connectionFailed }
            let data = try await transport.exchange(relayPayload(sealed, stream: stream))
            let plain = try await cipher.open(relayBytes(data, stream: stream))
            try Task.checkCancellation()
            guard self.cipher != nil else { throw MobileError.connectionFailed }
            return try WireReply.decode(plain, expectedID: id)
        } catch {
            close()
            try Task.checkCancellation()
            throw relayConnectionError(error)
        }
    }
}

private struct RelayEnvelope: Decodable {
    let type: String
    let requestID: String?
    let machineID: String?
    let streamID: String?
    let data: String?
    let error: String?
    enum CodingKeys: String, CodingKey { case type, requestID = "request_id", machineID = "machine_id", streamID = "stream_id", data, error }
}

private func relayPayload(_ data: Data, stream: String) throws -> Data {
    guard !data.isEmpty, data.count <= 256 * 1024 else { throw RelaySessionError.oversized }
    return try JSONSerialization.data(withJSONObject: ["type": "data", "stream_id": stream, "data": data.base64EncodedString()])
}

private func relayBytes(_ data: Data, stream: String) throws -> Data {
    let reply = try JSONDecoder().decode(RelayEnvelope.self, from: data)
    guard reply.streamID == stream else { throw MobileError.invalidResponse }
    if reply.type == "close" { throw MobileError.connectionFailed }
    guard reply.type == "data", let encoded = reply.data, let bytes = Data(base64Encoded: encoded),
          !bytes.isEmpty, bytes.count <= 256 * 1024, bytes.base64EncodedString() == encoded else { throw MobileError.invalidResponse }
    return bytes
}

private func relayConnectionError(_ error: any Error) -> any Error {
    if error is MobileError || error is RelaySessionError || error is CancellationError { return error }
    if error is DecodingError { return MobileError.invalidResponse }
    // URLSession may fail during send, before readMessage can map a closed connection.
    return MobileError.connectionFailed
}

/// Sliding budget is conservative against the server's fixed 120-frame/60s windows.
struct RelayFrameBudget {
    private var sent: [TimeInterval] = []
    var lastReservation: TimeInterval? { sent.last }
    mutating func reserve(at now: TimeInterval, frames: Int) -> TimeInterval {
        // 108 usable frames leave 120 - 108 = 12 frames for Mac registration/control traffic.
        precondition((1...108).contains(frames))
        sent.removeAll { $0 + 60 <= now }
        guard sent.count + frames <= 108 else {
            return max(0, sent[sent.count + frames - 109] + 60 - now)
        }
        sent.append(contentsOf: repeatElement(now, count: frames))
        return 0
    }
}

private actor RelayRequestBudget {
    static let shared = RelayRequestBudget()
    private var machines: [String: RelayFrameBudget] = [:]
    func reserve(machineID: String, frames: Int) async throws {
        while true {
            try Task.checkCancellation()
            let now = ProcessInfo.processInfo.systemUptime
            machines = machines.filter { ($0.value.lastReservation ?? 0) + 60 > now }
            let wait = machines[machineID, default: RelayFrameBudget()].reserve(at: now, frames: frames)
            if wait == 0 { return }
            // The caller is cancellable while waiting; recheck capacity after actor reentrancy.
            try await Task.sleep(for: .seconds(wait))
        }
    }
}
