import Foundation

/// A separate read-only connection prevents terminal refreshes from occupying the chat RPC.
public actor WorkspaceTerminalClient {
    private let machine: PairedMachine
    private let mobile: MobileClient
    private let relayFactory: @Sendable () async throws -> CollaborationRelayConnection
    private var direct: PinnedConnection?
    private var relay: CollaborationRelayConnection?
    private var remote = false
    private var busy = false
    public var refreshDelay: Duration { remote ? .seconds(1) : .milliseconds(250) }
    public init(machine: PairedMachine, mobile: MobileClient,
                relay: @escaping @Sendable () async throws -> CollaborationRelayConnection) {
        self.machine = machine; self.mobile = mobile; relayFactory = relay
    }
    public func close() async { direct?.close(); direct = nil; await relay?.close(); relay = nil; remote = false }
    public func frame(_ original: WorkspaceTerminalRequest) async throws -> WorkspaceTerminalFrame? {
        try original.validate()
        // Serialize whole captures across actor suspension, including history and all fragments.
        while busy { try await Task.sleep(for: .milliseconds(20)) }
        busy = true
        defer { busy = false }
        do { return try await capture(original) }
        catch {
            try Task.checkCancellation()
            // Switch only for transport failure. Scope, authorization and protocol errors remain terminal.
            if error as? MobileError == .connectionFailed || error as? MobileError == .timedOut {
                direct?.close(); direct = nil
                if remote { await relay?.close(); relay = nil }
                relay = try await relayFactory(); remote = true
                return try await capture(original)
            }
            throw error
        }
    }
    private func capture(_ original: WorkspaceTerminalRequest) async throws -> WorkspaceTerminalFrame? {
        var request = original
        var assembly = WorkspaceTerminalAssembly()
        while true {
            try Task.checkCancellation()
            let reply = try await rpc(request)
            guard reply.result?.status == "workspace_terminal", let response = reply.result?.workspaceTerminal,
                  response.request == request, validID(response.update.version) else { throw MobileError.invalidResponse }
            guard let fragment = response.update.fragment else {
                guard request.snapshotID == nil, request.before == nil, request.knownVersion == response.update.version else { throw MobileError.invalidResponse }
                return nil
            }
            guard fragment.frame.version == response.update.version else { throw MobileError.invalidResponse }
            if let complete = try assembly.apply(fragment) {
                let end = request.before ?? complete.totalLines
                guard complete.startLine == max(0, end - 120), complete.rows.count == end - complete.startLine,
                      request.before == nil || complete.cursor == nil else { throw MobileError.invalidResponse }
                return complete
            }
            request.snapshotID = fragment.snapshotID; request.offset = fragment.nextOffset
        }
    }
    private func rpc(_ request: WorkspaceTerminalRequest) async throws -> WireReply {
        let fields = try JSONSerialization.jsonObject(with: JSONEncoder().encode(request))
        if remote {
            guard let relay, await relay.capabilities.contains("workspace_terminal_v1") else { throw MobileError.unsupportedVersion }
            return try await relay.exchange(type: "workspace_terminal", fields: ["request":fields])
        }
        guard let credential = try await mobile.vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        if direct == nil {
            try machine.validate()
            let connection = try await PinnedConnection.open(machine, streaming: true)
            do {
                let authorized = try await exchange(connection, action: ["type":"authenticate", "device_id":credential.deviceID, "token":credential.token])
                guard let grant = authorized.result?.grant, authorized.result?.status == "authorized" else { throw MobileError.invalidResponse }
                try grant.validate(deviceID: credential.deviceID)
                guard authorized.result?.capabilities?.contains("workspace_terminal_v1") == true else { throw MobileError.unsupportedVersion }
                direct = connection
            } catch { connection.close(); throw error }
        }
        guard let direct else { throw MobileError.connectionFailed }
        return try await exchange(direct, action: ["type":"workspace_terminal", "device_id":credential.deviceID, "token":credential.token, "request":fields])
    }
    private func exchange(_ connection: PinnedConnection, action: [String: Any]) async throws -> WireReply {
        let id = UUID().uuidString
        let body = try JSONSerialization.data(withJSONObject: ["version":1,"request_id":id,"machine_id":machine.id,"action":action])
        let data = try await connection.exchange(body)
        return try WireReply.decode(data, expectedID: id)
    }
}
