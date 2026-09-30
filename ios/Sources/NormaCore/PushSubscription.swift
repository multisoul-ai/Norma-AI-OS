import Foundation

public enum APNsEnvironment: String, Codable, Sendable { case sandbox, production }

public struct APNsRegistration: Codable, Sendable, Equatable, CustomStringConvertible, CustomDebugStringConvertible {
    public let token: String
    public let environment: APNsEnvironment
    public init(token: String, environment: APNsEnvironment) { self.token = token; self.environment = environment }
    public var description: String { "APNsRegistration(\(environment.rawValue), token: [redacted])" }
    public var debugDescription: String { description }
    public func validate() throws {
        // APNs token size is not fixed; match the Mac's bounded 16–256 byte hex representation.
        guard (32...512).contains(token.utf8.count), token.utf8.count % 2 == 0,
              token.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else {
            throw MobileError.invalidPushRegistration
        }
    }
}

public struct PushSubscriptionState: Decodable, Sendable, Equatable {
    public let machineID: String
    public let deviceID: String
    public let revision: UInt64
    public let registration: APNsRegistration?
    public let registeredAtMilliseconds: UInt64?
    enum CodingKeys: String, CodingKey { case machineID = "machine_id", deviceID = "device_id", revision, registration, registeredAtMilliseconds = "registered_at_ms" }
    public func validate() throws {
        guard machineID.hasPrefix("mac-"), validHex(String(machineID.dropFirst(4))), validID(deviceID),
              revision <= Int64.max else { throw MobileError.invalidResponse }
        if let registration {
            try registration.validate()
            guard revision > 0, let time = registeredAtMilliseconds, time > 0, time <= Int64.max else { throw MobileError.invalidResponse }
        } else if registeredAtMilliseconds != nil { throw MobileError.invalidResponse }
    }
}

extension MobileClient {
    public func pushSubscription(_ machine: PairedMachine) async throws -> PushSubscriptionState {
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try await pushSubscriptionExchange(machine, credential: credential, update: nil)
    }

    /// One explicit update. On a lost response, re-read state; do not replay an old enable or disable.
    public func updatePushSubscription(_ machine: PairedMachine, expected: PushSubscriptionState, registration: APNsRegistration?) async throws -> PushSubscriptionState {
        try validatePushSubscriptionUpdate(machine, expected: expected, registration: registration)
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try await updatePushSubscription(machine, credential: credential, expected: expected, registration: registration)
    }

    /// Capture only in memory, before local Keychain deletion; remote cleanup never gates forgetting.
    public func preparePushSubscriptionRemoval(_ machine: PairedMachine) async throws -> @Sendable () async throws -> Void {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return {
            let current = try await pushSubscriptionExchange(machine, credential: credential, update: nil)
            if current.registration != nil {
                _ = try await updatePushSubscription(machine, credential: credential, expected: current, registration: nil)
            }
        }
    }

    private func validatePushSubscriptionUpdate(_ machine: PairedMachine, expected: PushSubscriptionState, registration: APNsRegistration?) throws {
        try expected.validate()
        guard expected.machineID == machine.id else { throw MobileError.wrongComputer }
        guard expected.revision < Int64.max else { throw MobileError.invalidResponse }
        try registration?.validate()
    }

    private func updatePushSubscription(_ machine: PairedMachine, credential: DeviceCredential, expected: PushSubscriptionState, registration: APNsRegistration?) async throws -> PushSubscriptionState {
        try validatePushSubscriptionUpdate(machine, expected: expected, registration: registration)
        let value: Any = try registration.map { try JSONSerialization.jsonObject(with: JSONEncoder().encode($0)) } ?? NSNull()
        let state = try await pushSubscriptionExchange(machine, credential: credential, update: ["expected_revision": expected.revision, "registration": value], expectedDeviceID: expected.deviceID)
        guard state.revision == expected.revision + 1, state.registration == registration else { throw MobileError.invalidResponse }
        return state
    }

    private func pushSubscriptionExchange(_ machine: PairedMachine, credential: DeviceCredential, update: [String: Any]?, expectedDeviceID: String? = nil) async throws -> PushSubscriptionState {
        try machine.validate()
        guard expectedDeviceID == nil || expectedDeviceID == credential.deviceID else { throw MobileError.invalidCredential }
        var action: [String: Any] = ["type": update == nil ? "push_subscription" : "set_push_subscription",
                                     "device_id": credential.deviceID, "token": credential.token]
        if let update { action.merge(update) { _, new in new } }
        let reply = try await exchange(machine, action: action)
        guard reply.result?.status == "push_subscription", let state = reply.result?.subscription else { throw MobileError.invalidResponse }
        try state.validate()
        guard state.machineID == machine.id else { throw MobileError.wrongComputer }
        guard state.deviceID == credential.deviceID else { throw MobileError.invalidCredential }
        return state
    }
}
