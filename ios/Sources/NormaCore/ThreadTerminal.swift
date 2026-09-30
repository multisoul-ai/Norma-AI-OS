import Foundation

struct ThreadTerminalRequest: Codable, Equatable, Sendable {
    let collaborationWorkspaceID: String
    let threadID: String
    let operationID: String
    let taskRef: String
    var before: Int?
    var revision: UInt64?
    enum CodingKeys: String, CodingKey {
        case collaborationWorkspaceID = "collaboration_workspace_id", threadID = "thread_id",
             operationID = "operation_id", taskRef = "task_ref", before, revision
    }
    init(task: ThreadTaskOperation, receipt: ThreadTaskReceipt) throws {
        guard receipt.receipt.state == "succeeded", let reference = receipt.receipt.result?.taskRef else {
            throw MobileError.terminalUnavailable
        }
        collaborationWorkspaceID = task.collaborationWorkspaceID; threadID = task.threadID
        operationID = task.operationID; taskRef = reference
    }
}

/// A page from the original Provider turn, never a snapshot of whichever task now occupies the node.
public struct ThreadTaskTerminal: Decodable, Sendable {
    let request: ThreadTerminalRequest
    public let scope: TerminalRequest
    public let key: MobileTurnOutcome.Key
    public let page: TerminalPage

    func validate(for expected: ThreadTerminalRequest, task: ThreadTaskOperation, receipt: ThreadTaskReceipt, controlled: Bool = false) throws {
        try key.validate(); try page.validate(for: scope)
        guard request == expected, request.collaborationWorkspaceID == task.collaborationWorkspaceID,
              request.threadID == task.threadID, request.operationID == task.operationID,
              request.taskRef == receipt.receipt.result?.taskRef,
              scope.workspaceID == task.projectID, scope.routeID == receipt.receipt.routeID,
              scope.before == request.before, scope.revision == request.revision,
              scope.nodeID == receipt.receipt.result?.nodeID, key.nodeID == scope.nodeID,
              key.runID == scope.runID, key.sessionID == scope.sessionID, key.profile == page.card.profile,
              (controlled || page.card.turnID == nil || page.card.turnID == key.turnID), controlled || !page.inputReady else {
            throw MobileError.invalidResponse
        }
        if let delivery = receipt.execution?.execution.delivery, delivery.outcome.key != key {
            throw MobileError.sessionChanged
        }
    }
}

extension CollaborationRelayConnection {
    public func threadTaskTerminal(task: ThreadTaskOperation, receipt: ThreadTaskReceipt,
                                   before previous: ThreadTaskTerminal? = nil) async throws -> ThreadTaskTerminal {
        guard capabilities.contains("thread_task_terminal_v1") else { throw ThreadTaskError.terminalRequiresUpgrade }
        guard let grant else { throw MobileError.unauthorized }
        try receipt.validate(task: task, machineID: machineID, deviceID: grant.deviceID)
        var request = try ThreadTerminalRequest(task: task, receipt: receipt)
        if let previous {
            try previous.validate(for: previous.request, task: task, receipt: receipt)
            guard previous.page.startLine > 0 else { throw MobileError.invalidResponse }
            request.before = previous.page.startLine; request.revision = previous.page.revision
        }
        do {
            try Task.checkCancellation()
            let fields = try JSONSerialization.jsonObject(with: JSONEncoder().encode(request))
            let reply = try await exchange(type: "thread_task_terminal", fields: ["request": fields])
            guard reply.result?.status == "thread_task_terminal", let terminal = reply.result?.threadTerminal else {
                throw MobileError.invalidResponse
            }
            try terminal.validate(for: request, task: task, receipt: receipt)
            if let previous, previous.key != terminal.key { throw MobileError.sessionChanged }
            return terminal
        } catch { close(); throw error }
    }
}
