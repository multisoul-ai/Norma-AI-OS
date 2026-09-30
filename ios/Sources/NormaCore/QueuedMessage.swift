import Foundation

/// Live queue state is separate from the immutable acknowledgement of durable handoff.
public struct QueuedMessageStatus: Codable, Equatable, Sendable {
    public let deviceID: String
    public let operationID: String
    public let workspaceID: String
    public let routeID: String
    public let nodeID: UInt64
    public let profile: String
    public let runID: String
    public let sessionID: String
    public let text: String
    public let state: String
    public let result: MobileCommandReceipt.CommandResult?
    public let error: String?
    public var isTerminal: Bool { ["sent", "cancelled", "outcome_unknown"].contains(state) }
    enum CodingKeys: String, CodingKey {
        case deviceID = "device_id", operationID = "operation_id", workspaceID = "workspace_id", routeID = "route_id"
        case nodeID = "node_id", runID = "run_id", sessionID = "session_id", profile, text, state, result, error
    }
    public func validate(for pending: PendingMobileCommand) throws {
        try pending.validate()
        guard case let .queueMessage(card, original) = pending.request.command,
              deviceID == pending.deviceID, operationID == pending.request.operationID,
              workspaceID == pending.request.workspaceID, routeID == pending.request.routeID,
              nodeID == card.nodeID, profile == card.profile, runID == card.runID,
              sessionID == card.sessionID, text == original else { throw MobileError.invalidResponse }
        switch state {
        case "queued", "dispatching": guard result == nil, error == nil else { throw MobileError.invalidResponse }
        case "sent":
            guard error == nil, let result, result.planID == nil, result.todoIndex == nil, result.nodeID == nodeID, result.runID == runID,
                  result.sessionID == sessionID, let turn = result.turnID, validID(turn), turn != card.turnID else { throw MobileError.invalidResponse }
        case "cancelled": guard result == nil, let error, validID(error), error != "outcome_unknown" else { throw MobileError.invalidResponse }
        case "outcome_unknown": guard result == nil, error == "outcome_unknown" else { throw MobileError.invalidResponse }
        default: throw MobileError.invalidResponse
        }
    }
}

extension MobileClient {
    public func prepareQueuedMessage(_ machine: PairedMachine, workspaceID: String, routeID: String, revision: UInt64, card: CardSnapshot, text: String, draftID: String? = nil) async throws -> PendingMobileCommand {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try PendingMobileCommand(machineID: machine.id, deviceID: credential.deviceID, workspaceID: workspaceID, routeID: routeID, revision: revision, queuing: card, text: text, draftID: draftID)
    }
    public func queuedMessage(_ pending: PendingMobileCommand, on machine: PairedMachine) async throws -> QueuedMessageStatus {
        try await queueExchange(pending, machine: machine, cancelRevision: nil)
    }
    public func cancelQueuedMessage(_ pending: PendingMobileCommand, on machine: PairedMachine, revision: UInt64) async throws -> QueuedMessageStatus {
        guard revision > 0 else { throw MobileError.invalidResponse }
        return try await queueExchange(pending, machine: machine, cancelRevision: revision)
    }
    private func queueExchange(_ pending: PendingMobileCommand, machine: PairedMachine, cancelRevision: UInt64?) async throws -> QueuedMessageStatus {
        try pending.validate(); try machine.validate()
        guard pending.request.command.isQueuedMessage else { throw MobileError.invalidResponse }
        guard machine.id == pending.machineID, let credential = try await vault.load(machineID: machine.id), credential.deviceID == pending.deviceID else { throw MobileError.invalidCredential }
        var action: [String: Any] = ["type": cancelRevision == nil ? "queued_message" : "cancel_queued_message",
            "device_id": credential.deviceID, "token": credential.token,
            "workspace_id": pending.request.workspaceID, "operation_id": pending.request.operationID]
        if let cancelRevision { action["grant_revision"] = cancelRevision }
        let reply = try await exchange(machine, action: action)
        guard reply.result?.status == "queued_message", let status = reply.result?.queuedMessage else { throw MobileError.invalidResponse }
        try status.validate(for: pending)
        return status
    }
}
