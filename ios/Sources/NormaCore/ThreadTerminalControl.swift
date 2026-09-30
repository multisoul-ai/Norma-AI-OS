import Foundation

/// Persist an explicit human action before sending; reads never enter this durable record.
public struct ThreadTerminalAction: Codable, Equatable, Sendable {
    public enum Kind: String, Codable, Sendable { case claim, input, release }
    public let type: Kind
    public let operationID: String
    public let claimOperationID: String?
    public let expected: CardSnapshot?
    public let screenSignature: UInt64?
    public let inputSequence: UInt64?
    public let input: TerminalInput?
    public var claimID: String { claimOperationID ?? operationID }
    enum CodingKeys: String, CodingKey {
        case type, operationID = "operation_id", claimOperationID = "claim_operation_id", expected,
             screenSignature = "screen_signature", inputSequence = "input_sequence", input
    }
    public init(operationID: String = UUID().uuidString) {
        type = .claim; self.operationID = operationID; claimOperationID = nil
        expected = nil; screenSignature = nil; inputSequence = nil; input = nil
    }
    public init(claimOperationID: String, page: TerminalPage, input: TerminalInput, operationID: String = UUID().uuidString) throws {
        guard page.inputReady else { throw MobileError.terminalChanged }
        type = .input; self.operationID = operationID; self.claimOperationID = claimOperationID
        expected = page.card; screenSignature = page.screenSignature; inputSequence = page.inputSequence; self.input = input
        try validate()
    }
    public static func release(claimOperationID: String, operationID: String = UUID().uuidString) -> Self {
        Self(type: .release, operationID: operationID, claimOperationID: claimOperationID)
    }
    private init(type: Kind, operationID: String, claimOperationID: String) {
        self.type = type; self.operationID = operationID; self.claimOperationID = claimOperationID
        expected = nil; screenSignature = nil; inputSequence = nil; input = nil
    }
    func validate() throws {
        guard UUID(uuidString: operationID) != nil,
              type == .claim ? claimOperationID == nil : (claimOperationID.flatMap(UUID.init(uuidString:)) != nil && claimOperationID != operationID)
        else { throw MobileError.invalidResponse }
        if type == .input {
            guard let expected, expected.runID != nil, expected.sessionID != nil,
                  screenSignature != nil, let inputSequence, inputSequence < UInt64.max, let input else { throw MobileError.invalidResponse }
            try expected.validate(); try input.validate()
        } else {
            guard expected == nil, screenSignature == nil, inputSequence == nil, input == nil else { throw MobileError.invalidResponse }
        }
    }
}

public struct ThreadTerminalState: Codable, Equatable, Sendable {
    public let action: ThreadTerminalAction
    public let key: MobileTurnOutcome.Key
    public let draftRevision: String?
    public internal(set) var receipt: MobileCommandReceipt?
    public internal(set) var outcomeAcknowledged: Bool? = nil
    /// Unknown input outcomes also require human reconciliation; they must never be replayed.
    public var unresolved: Bool {
        switch receipt?.state {
        case "succeeded", "rejected": false
        case "accepted": true
        default: outcomeAcknowledged != true
        }
    }

    func validate(for operation: ThreadTaskOutboxOperation) throws {
        try action.validate(); try key.validate()
        guard let original = operation.receipt, let device = operation.deviceID,
              original.receipt.state == "succeeded", key.nodeID == original.receipt.result?.nodeID,
              ![operation.id, operation.task.sourceOperationID, operation.task.sourceMessageID].contains(action.operationID),
              draftRevision.map({ UUID(uuidString: $0) != nil }) ?? true else { throw MobileError.invalidResponse }
        try original.validate(task: operation.task, machineID: operation.machineID, deviceID: device)
        if let delivery = original.execution?.execution.delivery, delivery.outcome.key != key { throw MobileError.sessionChanged }
        if let expected = action.expected {
            guard expected.nodeID == key.nodeID, expected.profile == key.profile,
                  expected.runID == key.runID, expected.sessionID == key.sessionID else { throw MobileError.sessionChanged }
        }
        if draftRevision != nil {
            guard case .text = action.input else { throw MobileError.invalidResponse }
        }
        guard let receipt else { return }
        guard receipt.deviceID == device, receipt.operationID == action.operationID,
              receipt.workspaceID == operation.task.projectID, receipt.routeID == original.receipt.routeID else { throw MobileError.invalidResponse }
        switch receipt.state {
        case "accepted": guard receipt.result == nil, receipt.error == nil else { throw MobileError.invalidResponse }
        case "succeeded":
            let status = action.type == .claim ? "controlled" : (action.type == .input ? "written" : "released")
            guard let result = receipt.result, receipt.error == nil, result.status == status,
                  result.key == key, result.taskRef == original.receipt.result?.taskRef,
                  result.nodeID == nil, result.runID == nil, result.sessionID == nil, result.turnID == nil,
                  result.feedback == nil, result.fingerprint == nil, result.acceptedAtUnix == nil,
                  result.todoIndex == nil, result.planID == nil, result.queueID == nil,
                  result.requestID == nil, result.decision == nil else { throw MobileError.invalidResponse }
        case "rejected":
            guard receipt.result == nil, let error = receipt.error, validID(error), error != "outcome_unknown" else { throw MobileError.invalidResponse }
        case "outcome_unknown":
            guard receipt.result == nil, receipt.error == "outcome_unknown" else { throw MobileError.invalidResponse }
        default: throw MobileError.invalidResponse
        }
    }
}

extension CollaborationRelayConnection {
    /// Caller saves the action first. Reconnection must use threadTerminalReceipt, never this method.
    public func submitThreadTerminalAction(for operation: ThreadTaskOutboxOperation) async throws -> MobileCommandReceipt {
        try await terminalControl(operation, lookupOnly: false)
    }
    public func threadTerminalReceipt(for operation: ThreadTaskOutboxOperation) async throws -> MobileCommandReceipt {
        try await terminalControl(operation, lookupOnly: true)
    }
    private func terminalControl(_ operation: ThreadTaskOutboxOperation, lookupOnly: Bool) async throws -> MobileCommandReceipt {
        guard capabilities.contains("thread_task_terminal_control_v1") else { throw MobileError.terminalUnavailable }
        guard let grant, operation.machineID == machineID, operation.deviceID == grant.deviceID,
              var state = operation.terminalOperation, let receipt = operation.receipt else { throw MobileError.invalidCredential }
        try state.validate(for: operation)
        let request = try ThreadTerminalRequest(task: operation.task, receipt: receipt)
        do {
            let reply = try await exchange(type: "thread_task_terminal_control", fields: [
                "request": JSONSerialization.jsonObject(with: JSONEncoder().encode(request)),
                "operation": JSONSerialization.jsonObject(with: JSONEncoder().encode(state.action)), "lookup_only": lookupOnly])
            guard reply.result?.status == "receipt", let received = reply.result?.receipt else { throw MobileError.invalidResponse }
            state.receipt = received; try state.validate(for: operation)
            return received
        } catch { close(); throw error }
    }

    /// Server validates this connection's lease. A stored successful claim cannot recreate ownership.
    public func controlledThreadTerminal(for operation: ThreadTaskOutboxOperation, claimOperationID: String,
                                         before previous: ThreadTaskTerminal? = nil) async throws -> ThreadTaskTerminal {
        guard capabilities.contains("thread_task_terminal_control_v1") else { throw MobileError.terminalUnavailable }
        guard let grant, operation.machineID == machineID, operation.deviceID == grant.deviceID,
              let state = operation.terminalOperation, state.action.claimID == claimOperationID,
              let receipt = operation.receipt else { throw MobileError.invalidCredential }
        try state.validate(for: operation)
        var request = try ThreadTerminalRequest(task: operation.task, receipt: receipt)
        if let previous {
            try previous.validate(for: previous.request, task: operation.task, receipt: receipt, controlled: true)
            guard previous.key == state.key, previous.page.startLine > 0 else { throw MobileError.invalidResponse }
            request.before = previous.page.startLine; request.revision = previous.page.revision
        }
        do {
            let reply = try await exchange(type: "thread_task_terminal_control", fields: [
                "request": JSONSerialization.jsonObject(with: JSONEncoder().encode(request)),
                "operation": ["type": "read", "claim_operation_id": claimOperationID]])
            guard reply.result?.status == "thread_task_terminal", let terminal = reply.result?.threadTerminal,
                  terminal.key == state.key else { throw MobileError.invalidResponse }
            try terminal.validate(for: request, task: operation.task, receipt: receipt, controlled: true)
            return terminal
        } catch { close(); throw error }
    }
}
