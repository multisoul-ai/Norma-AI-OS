import Foundation

public struct TodoDecisionRef: Codable, Sendable, Equatable {
    public let deviceID: String
    public let operationID: String
    enum CodingKeys: String, CodingKey { case deviceID = "device_id", operationID = "operation_id" }
}

public enum PlanTodoChoice: Codable, Sendable, Equatable {
    case accept(target: CardSnapshot, queue: Bool)
    case reject
    enum CodingKeys: String, CodingKey { case type, target, queue }
    public init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        switch try values.decode(String.self, forKey: .type) {
        case "accept": self = .accept(target: try values.decode(CardSnapshot.self, forKey: .target), queue: try values.decode(Bool.self, forKey: .queue))
        case "reject": self = .reject
        default: throw MobileError.invalidResponse
        }
    }
    public func encode(to encoder: any Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .accept(target, queue):
            try values.encode("accept", forKey: .type); try values.encode(target, forKey: .target); try values.encode(queue, forKey: .queue)
        case .reject: try values.encode("reject", forKey: .type)
        }
    }
}

extension ProjectPlan.Todo {
    public func acceptance(for current: CardSnapshot?) -> PlanTodoChoice? {
        guard let current else { return nil }
        let choice = PlanTodoChoice.accept(target: current, queue: current.isBusy)
        do { try validate(choice: choice); return choice }
        catch { return nil }
    }

    func validate(choice: PlanTodoChoice) throws {
        try target.validate()
        guard target.mobileControl, MobileAgentProfile.supports(target.profile),
              validPlanText(title, maxBytes: 320), validPlanText(instruction, maxBytes: 8 * 1024) else { throw MobileError.invalidResponse }
        if case let .accept(current, queue) = choice {
            try current.validate()
            guard current.nodeID == target.nodeID, current.profile == target.profile, current.mobileControl,
                  current.sessionID == nil || current.runID != nil else { throw MobileError.invalidResponse }
            if queue {
                guard current.runID != nil, current.sessionID != nil, !["unknown", "preparing"].contains(current.state) else { throw MobileError.invalidResponse }
            } else {
                guard ["idle", "ready", "done", "ended", "failed"].contains(current.state) else { throw MobileError.invalidResponse }
            }
        }
    }
}

extension PendingMobileCommand {
    public init(machineID: String, deviceID: String, routeID: String, revision: UInt64, deciding plan: ProjectPlan, todoIndex: Int, choice: PlanTodoChoice, retryOf: TodoDecisionRef? = nil, operationID: String = UUID().uuidString) throws {
        try plan.validate(workspaceID: plan.workspaceID)
        guard plan.draft.todos.indices.contains(todoIndex) else { throw MobileError.invalidResponse }
        self.machineID = machineID; self.deviceID = deviceID; todoPreview = plan.draft.todos[todoIndex]
        request = Request(operationID: operationID, workspaceID: plan.workspaceID, routeID: routeID, grantRevision: revision, command: .decidePlanTodo(planID: plan.planID, todoIndex: todoIndex, choice: choice, retryOf: retryOf))
        try validate()
    }
    /// Status validation only. Submitting this rewritten command would conflict with the saved fingerprint.
    func todoMessageCommand() throws -> PendingMobileCommand {
        guard case let .decidePlanTodo(_, _, .accept(target, queue), _) = request.command,
              let preview = todoPreview else { throw MobileError.invalidResponse }
        if queue {
            return try PendingMobileCommand(machineID: machineID, deviceID: deviceID, workspaceID: request.workspaceID, routeID: request.routeID, revision: request.grantRevision, queuing: target, text: preview.instruction, operationID: request.operationID)
        }
        return try PendingMobileCommand(machineID: machineID, deviceID: deviceID, workspaceID: request.workspaceID, routeID: request.routeID, revision: request.grantRevision, sending: target, text: preview.instruction, operationID: request.operationID)
    }
}

public struct MobileTurnOutcome: Decodable, Sendable {
    public enum State: String, Decodable, Sendable { case completed, failed, interrupted }
    public struct Key: Codable, Sendable, Equatable {
        public let nodeID: UInt64
        public let profile: String
        public let runID: String
        public let sessionID: String
        public let turnID: String
        enum CodingKeys: String, CodingKey { case nodeID = "node_id", profile, runID = "run_id", sessionID = "session_id", turnID = "turn_id" }
    }
    public let key: Key
    public let state: State
}

public struct ProjectTodoDecision: Decodable, Sendable, Identifiable {
    public let todoIndex: Int
    public let choice: PlanTodoChoice
    public let receipt: MobileCommandReceipt
    public let queuedMessage: QueuedMessageStatus?
    public let outcome: MobileTurnOutcome?
    public let retryOf: TodoDecisionRef?
    private let retryAllowed: Bool?
    public var canRetry: Bool { retryAllowed == true }
    public var reference: TodoDecisionRef { TodoDecisionRef(deviceID: receipt.deviceID, operationID: receipt.operationID) }
    public var id: Int { todoIndex }
    /// Completion leaves the pending list, but the immutable decision remains available for review.
    public var needsFollowUp: Bool { deliveryState != "declined" && outcome?.state != .completed }
    public var deliveryState: String {
        if let queuedMessage { return queuedMessage.state }
        if case .reject = choice, receipt.state == "succeeded" { return "declined" }
        switch receipt.state {
        case "accepted": return "dispatching"
        case "succeeded": return "sent"
        case "rejected": return "not_sent"
        default: return "outcome_unknown"
        }
    }
    public var deliveryResult: MobileCommandReceipt.CommandResult? { queuedMessage?.result ?? receipt.result }
    public func conversationCard(in project: ProjectSnapshot) -> CardSnapshot? {
        guard deliveryState == "sent", let result = deliveryResult, result.turnID != nil,
              project.id == receipt.workspaceID, project.routeID == receipt.routeID,
              let card = project.cards?.first(where: { $0.id == result.nodeID }),
              card.runID == result.runID, card.sessionID == result.sessionID,
              case let .accept(target, _) = choice, card.profile == target.profile else { return nil }
        return card
    }
    enum CodingKeys: String, CodingKey { case retryOf = "retry_of", retryAllowed = "can_retry", todoIndex = "todo_index", choice, receipt, outcome, queuedMessage = "queued_message" }
    func validate(plan: ProjectPlan, machineID: String) throws {
        let command = try PendingMobileCommand(machineID: machineID, deviceID: receipt.deviceID, routeID: receipt.routeID, revision: 1, deciding: plan, todoIndex: todoIndex, choice: choice, retryOf: retryOf, operationID: receipt.operationID)
        try receipt.validate(for: command)
        if case .accept(_, true) = choice {
            guard let queuedMessage else { throw MobileError.invalidResponse }
            try queuedMessage.validate(for: command.todoMessageCommand())
        } else if queuedMessage != nil { throw MobileError.invalidResponse }
        if canRetry {
            guard case .accept = choice, ["not_sent", "cancelled"].contains(deliveryState), outcome == nil else { throw MobileError.invalidResponse }
        }
        if let outcome {
            guard deliveryState == "sent", let result = deliveryResult, case let .accept(target, _) = choice,
                  outcome.key.nodeID == result.nodeID, outcome.key.profile == target.profile,
                  outcome.key.runID == result.runID, outcome.key.sessionID == result.sessionID,
                  outcome.key.turnID == result.turnID else { throw MobileError.invalidResponse }
        }
    }
}

public struct ProjectPlanProgress: Decodable, Sendable {
    public let plan: ProjectPlan
    public let decisions: [ProjectTodoDecision]
    func validate(workspaceID: String, planID: String, machineID: String) throws {
        try plan.validate(workspaceID: workspaceID)
        guard plan.planID == planID, decisions.count <= plan.draft.todos.count,
              Set(decisions.map(\.id)).count == decisions.count else { throw MobileError.invalidResponse }
        try decisions.forEach { try $0.validate(plan: plan, machineID: machineID) }
    }
}

extension MobileClient {
    public func prepareTodo(_ machine: PairedMachine, routeID: String, revision: UInt64, plan: ProjectPlan, todoIndex: Int, choice: PlanTodoChoice, retryOf: TodoDecisionRef? = nil) async throws -> PendingMobileCommand {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try PendingMobileCommand(machineID: machine.id, deviceID: credential.deviceID, routeID: routeID, revision: revision, deciding: plan, todoIndex: todoIndex, choice: choice, retryOf: retryOf)
    }
    public func projectPlan(_ machine: PairedMachine, workspaceID: String, planID: String) async throws -> ProjectPlanProgress {
        try machine.validate()
        guard validID(workspaceID), validID(planID) else { throw MobileError.invalidResponse }
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        let reply = try await exchange(machine, action: ["type": "project_plan", "device_id": credential.deviceID, "token": credential.token, "workspace_id": workspaceID, "plan_id": planID])
        guard reply.result?.status == "project_plan", reply.result?.workspaceID == workspaceID,
              let plan = reply.result?.plan, let decisions = reply.result?.decisions else { throw MobileError.invalidResponse }
        let progress = ProjectPlanProgress(plan: plan, decisions: decisions)
        try progress.validate(workspaceID: workspaceID, planID: planID, machineID: machine.id)
        return progress
    }
    public func cancelTodoQueue(_ pending: PendingMobileCommand, on machine: PairedMachine, revision: UInt64) async throws -> QueuedMessageStatus {
        try pending.validate()
        return try await cancelQueuedMessage(pending.todoMessageCommand(), on: machine, revision: revision)
    }
}
