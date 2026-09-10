import Foundation

public enum PairingConnection: Sendable {
    case authorized(DeviceGrant)
    case pending(PairingPending)
}

public struct MobileClient: Sendable {
    let vault: CredentialVault
    public init(vault: CredentialVault) { self.vault = vault }

    public func connect(_ offer: PairingOffer, deviceName: String) async throws -> PairingConnection {
        // A scan may update LAN routes for an already authorized computer. Keep its credential
        // and scope; a failed transport or identity check must never fall through to registration.
        if try await vault.load(machineID: offer.machineID) != nil {
            do { return .authorized(try await authenticate(PairedMachine(offer: offer))) }
            catch MobileError.unauthorized { /* An earlier pairing may still await Mac approval. */ }
        }
        try Task.checkCancellation()
        return .pending(try await pair(offer, deviceName: deviceName))
    }

    public func pair(_ offer: PairingOffer, deviceName: String) async throws -> PairingPending {
        guard validName(deviceName) else { throw MobileError.invalidOffer }
        let credential = try await vault.loadOrCreate(machineID: offer.machineID)
        let code = try verificationCode(offerID: offer.offerID, secret: offer.secret, credential: credential)
        let action = ["type": "pair", "request": ["offer_id": offer.offerID, "secret": offer.secret,
            "device_id": credential.deviceID, "device_name": deviceName, "device_token": credential.token]] as [String: Any]
        let reply = try await exchange(PairedMachine(offer: offer), action: action)
        guard reply.result?.status == "pending", let pending = reply.result?.pending,
              validID(pending.requestID), pending.deviceID == credential.deviceID,
              pending.deviceName == deviceName else { throw MobileError.invalidResponse }
        guard pending.verificationCode == code else { throw MobileError.verificationMismatch }
        return pending
    }

    public func authenticate(_ machine: PairedMachine) async throws -> DeviceGrant {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        let reply = try await exchange(machine, action: ["type": "authenticate",
            "device_id": credential.deviceID, "token": credential.token])
        guard reply.result?.status == "authorized", let grant = reply.result?.grant else { throw MobileError.invalidResponse }
        try grant.validate(deviceID: credential.deviceID)
        return grant
    }

    public func snapshot(_ machine: PairedMachine, workspaceID: String? = nil) async throws -> MachineSnapshot {
        try machine.validate()
        if let workspaceID, !validID(workspaceID) { throw MobileError.invalidResponse }
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        var action: [String: Any] = ["type": "snapshot", "device_id": credential.deviceID, "token": credential.token]
        if let workspaceID { action["workspace_id"] = workspaceID }
        let reply = try await exchange(machine, action: action)
        return try MachineSnapshot(reply: reply, deviceID: credential.deviceID, workspaceID: workspaceID)
    }

    func exchange(_ machine: PairedMachine, action: [String: Any], timeout: Duration = .seconds(5)) async throws -> WireReply {
        let id = UUID().uuidString
        let body = try JSONSerialization.data(withJSONObject: ["version": 1, "request_id": id,
            "machine_id": machine.id, "action": action])
        let connection = try await PinnedConnection.open(machine)
        defer { connection.close() }
        // Once a request may have been sent, do not switch endpoints or replay it.
        let data = try await connection.exchange(body, timeout: timeout)
        return try WireReply.decode(data, expectedID: id)
    }
}
