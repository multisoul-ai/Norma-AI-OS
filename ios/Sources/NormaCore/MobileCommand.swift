import Foundation

/// Saved before sending. Loading this file never authorizes resubmission.
public struct PendingMobileCommand: Codable, Sendable, Identifiable, Equatable {
    public let machineID: String
    public let deviceID: String
    public let request: Request
    /// Local editing revision; never included in the network Request.
    public var draftID: String? = nil
    public var todoPreview: ProjectPlan.Todo? = nil
    public var approvalPreview: ApprovalSnapshot? = nil
    public var id: String { machineID + ":" + deviceID + ":" + request.operationID }

    public struct Request: Codable, Sendable, Equatable {
        public let operationID: String
        public let workspaceID: String
        public let routeID: String
        public let grantRevision: UInt64
        public let command: Command
        enum CodingKeys: String, CodingKey {
            case operationID = "operation_id", workspaceID = "workspace_id", routeID = "route_id"
            case grantRevision = "grant_revision", command
        }
    }
    public enum Command: Codable, Sendable, Equatable {
        case terminalInput(expected: CardSnapshot, screenSignature: UInt64, inputSequence: UInt64, input: TerminalInput)
        case feedbackDelivery(expected: CardSnapshot, feedback: DeliveryFeedback)
        case acceptDelivery(key: MobileTurnOutcome.Key, fingerprint: String)
        case decidePlanTodo(planID: String, todoIndex: Int, choice: PlanTodoChoice, retryOf: TodoDecisionRef? = nil)
        case generatePlan(direction: String, priority: PlanPriority? = nil)
        case decideApproval(expected: ApprovalTarget, decision: ApprovalDecision)
        case createCard(profile: String, title: String?)
        case deleteCard(expected: CardSnapshot, confirmBusy: Bool)
        case sendMessage(expected: CardSnapshot, text: String)
        case queueMessage(expected: CardSnapshot, text: String)
        case interruptTurn(expected: CardSnapshot)
        enum CodingKeys: String, CodingKey { case input, screenSignature = "screen_signature", inputSequence = "input_sequence", feedback, key, fingerprint, retryOf = "retry_of", planID = "plan_id", todoIndex = "todo_index", choice, type, profile, title, expected, text, decision, direction, priority, confirmBusy = "confirm_busy" }
        public var isTerminal: Bool { if case .terminalInput = self { true } else { false } }
        public var feedback: DeliveryFeedback? { if case let .feedbackDelivery(_, feedback) = self { feedback } else { nil } }
        public var isAcceptance: Bool { if case .acceptDelivery = self { true } else { false } }
        public var isTodo: Bool { if case .decidePlanTodo = self { true } else { false } }
        public var isPlan: Bool { if case .generatePlan = self { true } else { false } }
        public var isApproval: Bool { if case .decideApproval = self { true } else { false } }
        public var isDeletion: Bool { if case .deleteCard = self { true } else { false } }
        public var isInterrupt: Bool { if case .interruptTurn = self { true } else { false } }
        public var isMessage: Bool { messageContent != nil }
        public var isQueuedMessage: Bool { if case .queueMessage = self { true } else { false } }
        public var messageContent: (card: CardSnapshot, text: String)? {
            switch self { case let .sendMessage(card, text), let .queueMessage(card, text): (card, text); case let .feedbackDelivery(card, feedback): (card, feedback.text); default: nil }
        }
        public var displayName: String {
            switch self { case .terminalInput: "终端输入"; case .feedbackDelivery: "成果修改意见"; case .acceptDelivery: "成果验收"; case .decidePlanTodo: "下一步决定"; case .generatePlan: "梳理下一步"; case let .decideApproval(target, _): target.profile + " 审批"; case let .createCard(profile, title): title ?? profile; case let .deleteCard(card, _), let .sendMessage(card, _), let .queueMessage(card, _), let .interruptTurn(card): card.title }
        }
        public init(from decoder: any Decoder) throws {
            let values = try decoder.container(keyedBy: CodingKeys.self)
            switch try values.decode(String.self, forKey: .type) {
            case "terminal_input": self = .terminalInput(expected: try values.decode(CardSnapshot.self, forKey: .expected), screenSignature: try values.decode(UInt64.self, forKey: .screenSignature), inputSequence: try values.decode(UInt64.self, forKey: .inputSequence), input: try values.decode(TerminalInput.self, forKey: .input))
            case "feedback_delivery": self = .feedbackDelivery(expected: try values.decode(CardSnapshot.self, forKey: .expected), feedback: try values.decode(DeliveryFeedback.self, forKey: .feedback))
            case "accept_delivery": self = .acceptDelivery(key: try values.decode(MobileTurnOutcome.Key.self, forKey: .key), fingerprint: try values.decode(String.self, forKey: .fingerprint))
            case "decide_plan_todo": self = .decidePlanTodo(planID: try values.decode(String.self, forKey: .planID), todoIndex: try values.decode(Int.self, forKey: .todoIndex), choice: try values.decode(PlanTodoChoice.self, forKey: .choice), retryOf: try values.decodeIfPresent(TodoDecisionRef.self, forKey: .retryOf))
            case "generate_plan": self = .generatePlan(direction: try values.decode(String.self, forKey: .direction), priority: try values.decodeIfPresent(PlanPriority.self, forKey: .priority))
            case "decide_approval": self = .decideApproval(expected: try values.decode(ApprovalTarget.self, forKey: .expected), decision: try values.decode(ApprovalDecision.self, forKey: .decision))
            case "create_card": self = .createCard(profile: try values.decode(String.self, forKey: .profile), title: try values.decodeIfPresent(String.self, forKey: .title))
            case "delete_card": self = .deleteCard(expected: try values.decode(CardSnapshot.self, forKey: .expected), confirmBusy: try values.decode(Bool.self, forKey: .confirmBusy))
            case "interrupt_turn": self = .interruptTurn(expected: try values.decode(CardSnapshot.self, forKey: .expected))
            case "queue_message": self = .queueMessage(expected: try values.decode(CardSnapshot.self, forKey: .expected), text: try values.decode(String.self, forKey: .text))
            case "send_message": self = .sendMessage(expected: try values.decode(CardSnapshot.self, forKey: .expected), text: try values.decode(String.self, forKey: .text))
            default: throw MobileError.invalidResponse
            }
        }
        public func encode(to encoder: any Encoder) throws {
            var values = encoder.container(keyedBy: CodingKeys.self)
            switch self {
            case let .terminalInput(expected, revision, sequence, input):
                try values.encode("terminal_input", forKey: .type); try values.encode(expected, forKey: .expected)
                try values.encode(revision, forKey: .screenSignature); try values.encode(sequence, forKey: .inputSequence); try values.encode(input, forKey: .input)
            case let .feedbackDelivery(expected, feedback):
                try values.encode("feedback_delivery", forKey: .type); try values.encode(expected, forKey: .expected); try values.encode(feedback, forKey: .feedback)
            case let .acceptDelivery(key, fingerprint):
                try values.encode("accept_delivery", forKey: .type); try values.encode(key, forKey: .key); try values.encode(fingerprint, forKey: .fingerprint)
            case let .decidePlanTodo(planID, index, choice, retryOf):
                try values.encode("decide_plan_todo", forKey: .type); try values.encode(planID, forKey: .planID)
                try values.encode(index, forKey: .todoIndex); try values.encode(choice, forKey: .choice)
                try values.encodeIfPresent(retryOf, forKey: .retryOf)
            case let .generatePlan(direction, priority):
                try values.encode("generate_plan", forKey: .type); try values.encode(direction, forKey: .direction)
                try values.encodeIfPresent(priority, forKey: .priority)
            case let .decideApproval(expected, decision):
                try values.encode("decide_approval", forKey: .type); try values.encode(expected, forKey: .expected); try values.encode(decision, forKey: .decision)
            case let .createCard(profile, title):
                try values.encode("create_card", forKey: .type); try values.encode(profile, forKey: .profile)
                try values.encodeIfPresent(title, forKey: .title)
            case let .deleteCard(expected, confirmBusy):
                try values.encode("delete_card", forKey: .type); try values.encode(expected, forKey: .expected)
                try values.encode(confirmBusy, forKey: .confirmBusy)
            case let .interruptTurn(expected):
                try values.encode("interrupt_turn", forKey: .type); try values.encode(expected, forKey: .expected)
            case let .sendMessage(expected, text), let .queueMessage(expected, text):
                try values.encode(isQueuedMessage ? "queue_message" : "send_message", forKey: .type); try values.encode(expected, forKey: .expected)
                try values.encode(text, forKey: .text)
            }
        }
    }
    public init(machineID: String, deviceID: String, workspaceID: String, routeID: String, revision: UInt64, profile: String, title: String?, operationID: String = UUID().uuidString) throws {
        self.machineID = machineID; self.deviceID = deviceID
        request = Request(operationID: operationID, workspaceID: workspaceID, routeID: routeID, grantRevision: revision, command: .createCard(profile: profile, title: title))
        try validate()
    }
    public init(machineID: String, deviceID: String, workspaceID: String, routeID: String, revision: UInt64, deleting card: CardSnapshot, confirmBusy: Bool, operationID: String = UUID().uuidString) throws {
        self.machineID = machineID; self.deviceID = deviceID
        request = Request(operationID: operationID, workspaceID: workspaceID, routeID: routeID, grantRevision: revision, command: .deleteCard(expected: card, confirmBusy: confirmBusy))
        try validate()
    }
    public init(machineID: String, deviceID: String, workspaceID: String, routeID: String, revision: UInt64, interrupting card: CardSnapshot, operationID: String = UUID().uuidString) throws {
        self.machineID = machineID; self.deviceID = deviceID
        request = Request(operationID: operationID, workspaceID: workspaceID, routeID: routeID, grantRevision: revision, command: .interruptTurn(expected: card))
        try validate()
    }
    func validate() throws {
        if let draftID, !validID(draftID) { throw MobileError.invalidResponse }
        guard machineID.hasPrefix("mac-"), validHex(String(machineID.dropFirst(4))),
              [deviceID, request.operationID, request.workspaceID, request.routeID].allSatisfy(validID),
              request.grantRevision > 0 else { throw MobileError.invalidResponse }
        if todoPreview != nil && !request.command.isTodo { throw MobileError.invalidResponse }
        if approvalPreview != nil && !request.command.isApproval { throw MobileError.invalidResponse }
        switch request.command {
        case let .terminalInput(card, _, sequence, input):
            try card.validate(); try input.validate()
            guard card.mobileControl, card.runID != nil, ["Codex", "Claude"].contains(card.profile), sequence < UInt64.max else { throw MobileError.invalidResponse }
        case let .feedbackDelivery(card, feedback):
            try feedback.validate(for: card)
        case let .acceptDelivery(key, fingerprint):
            try key.validate()
            guard validHex(fingerprint) else { throw MobileError.invalidResponse }
        case let .decidePlanTodo(planID, index, choice, retryOf):
            guard validID(planID), (0..<8).contains(index), let preview = todoPreview else { throw MobileError.invalidResponse }
            try preview.validate(choice: choice)
            if let retryOf {
                guard validID(retryOf.deviceID), validID(retryOf.operationID),
                      retryOf.deviceID != deviceID || retryOf.operationID != request.operationID,
                      case .accept = choice else { throw MobileError.invalidResponse }
            }
        case let .generatePlan(direction, priority):
            try priority?.validate()
            guard validPlanText(direction, maxBytes: 8 * 1024) else { throw MobileError.invalidResponse }
        case let .decideApproval(target, decision):
            try target.validate()
            guard let preview = approvalPreview, preview.target == target, preview.decisions.contains(decision) else { throw MobileError.invalidResponse }
            try preview.validate()
        case let .createCard(profile, title):
            guard ["Codex", "Claude"].contains(profile) else { throw MobileError.invalidResponse }
            if let title {
                guard !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, title.unicodeScalars.count <= 80,
                      !title.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains) else { throw MobileError.invalidResponse }
            }
        case let .deleteCard(card, confirmBusy):
            try card.validate()
            guard ["Codex", "Claude"].contains(card.profile), !["preparing", "unknown", "collecting"].contains(card.state),
                  !card.isBusy || (confirmBusy && card.runID != nil && card.sessionID != nil && card.turnID != nil) else { throw MobileError.invalidResponse }
        case let .interruptTurn(card):
            try card.validate()
            guard card.mobileControl, card.canInterrupt != false, ["Codex", "Claude"].contains(card.profile),
                  ["thinking", "running_tool", "waiting_approval"].contains(card.state),
                  let run = card.runID, let session = card.sessionID, let turn = card.turnID,
                  [run, session, turn].allSatisfy(validID) else { throw MobileError.invalidResponse }
        case let .sendMessage(card, text), let .queueMessage(card, text):
            try card.validate()
            if request.command.isQueuedMessage {
                guard !["unknown", "preparing"].contains(card.state), ["Codex", "Claude"].contains(card.profile),
                      let run = card.runID, let session = card.sessionID, [run, session].allSatisfy(validID) else { throw MobileError.invalidResponse }
            } else {
                guard ["idle", "ready", "done", "ended", "failed"].contains(card.state) else { throw MobileError.invalidResponse }
            }
            guard card.mobileControl,
                  !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, text.utf8.count <= 32 * 1024,
                  !text.unicodeScalars.contains(where: { ($0.value < 32 || (127...159).contains($0.value)) && $0.value != 9 && $0.value != 10 }) else { throw MobileError.invalidResponse }
        }
    }
    public init(machineID: String, deviceID: String, workspaceID: String, routeID: String, revision: UInt64, queuing card: CardSnapshot, text: String, draftID: String? = nil, operationID: String = UUID().uuidString) throws {
        self.machineID = machineID; self.deviceID = deviceID
        self.draftID = draftID
        request = Request(operationID: operationID, workspaceID: workspaceID, routeID: routeID, grantRevision: revision, command: .queueMessage(expected: card, text: text))
        try validate()
    }
    public init(machineID: String, deviceID: String, workspaceID: String, routeID: String, revision: UInt64, sending card: CardSnapshot, text: String, draftID: String? = nil, operationID: String = UUID().uuidString) throws {
        self.machineID = machineID; self.deviceID = deviceID
        self.draftID = draftID
        request = Request(operationID: operationID, workspaceID: workspaceID, routeID: routeID, grantRevision: revision, command: .sendMessage(expected: card, text: text))
        try validate()
    }
}

public struct MobileCommandReceipt: Decodable, Sendable {
    public let deviceID: String
    public let operationID: String
    public let workspaceID: String
    public let routeID: String
    public let state: String
    public let result: CommandResult?
    public let error: String?
    public struct CommandResult: Decodable, Sendable {
        public let feedback: DeliveryFeedback.Reference?
        public let fingerprint: String?
        public let acceptedAtUnix: UInt64?
        public let nodeID: UInt64?
        public let todoIndex: Int?
        public let planID: String?
        public let runID: String?
        public let sessionID: String?
        public let turnID: String?
        public let status: String?
        public let queueID: String?
        public let requestID: String?
        public let decision: ApprovalDecision?
        enum CodingKeys: String, CodingKey { case feedback, fingerprint, acceptedAtUnix = "accepted_at_unix", todoIndex = "todo_index", planID = "plan_id", nodeID = "node_id", runID = "run_id", sessionID = "session_id", turnID = "turn_id", queueID = "queue_id", requestID = "request_id", status, decision }
    }
    enum CodingKeys: String, CodingKey {
        case deviceID = "device_id", operationID = "operation_id", workspaceID = "workspace_id", routeID = "route_id", state, result, error
    }
    func validate(for pending: PendingMobileCommand) throws {
        try pending.validate()
        guard deviceID == pending.deviceID, operationID == pending.request.operationID,
              workspaceID == pending.request.workspaceID, routeID == pending.request.routeID else { throw MobileError.invalidResponse }
        if case let .decidePlanTodo(planID, index, choice, _) = pending.request.command {
            if case .accept = choice {
                try validate(for: pending.todoMessageCommand())
                return
            }
            if state == "succeeded" {
                guard error == nil, let result, result.planID == planID, result.todoIndex == index, result.status == "declined",
                      result.nodeID == nil, result.runID == nil, result.sessionID == nil, result.turnID == nil,
                      result.queueID == nil, result.requestID == nil, result.decision == nil else { throw MobileError.invalidResponse }
                return
            }
        }
        switch state {
        case "accepted": guard result == nil, error == nil else { throw MobileError.invalidResponse }
        case "succeeded":
            guard let result, error == nil else { throw MobileError.invalidResponse }
            if case let .terminalInput(card, _, _, _) = pending.request.command {
                guard result.nodeID == card.nodeID, result.runID == card.runID, result.sessionID == card.sessionID,
                      result.status == "written", result.turnID == nil, result.requestID == nil, result.decision == nil,
                      result.queueID == nil, result.feedback == nil, result.fingerprint == nil, result.acceptedAtUnix == nil,
                      result.planID == nil, result.todoIndex == nil else { throw MobileError.invalidResponse }
                return
            }
            if case let .feedbackDelivery(card, feedback) = pending.request.command {
                guard result.feedback == feedback.reference, result.nodeID == card.nodeID, result.runID == card.runID,
                      result.sessionID == card.sessionID, let turn = result.turnID, validID(turn), turn != feedback.key.turnID,
                      result.fingerprint == nil, result.acceptedAtUnix == nil, result.status == nil, result.planID == nil,
                      result.todoIndex == nil, result.queueID == nil, result.requestID == nil, result.decision == nil else { throw MobileError.invalidResponse }
                return
            }
            guard result.feedback == nil else { throw MobileError.invalidResponse }
            if case let .acceptDelivery(key, fingerprint) = pending.request.command {
                guard result.nodeID == key.nodeID, result.runID == key.runID, result.sessionID == key.sessionID,
                      result.turnID == key.turnID, result.fingerprint == fingerprint,
                      let acceptedAt = result.acceptedAtUnix, acceptedAt > 0, acceptedAt <= Int64.max,
                      result.status == nil, result.planID == nil, result.todoIndex == nil, result.queueID == nil,
                      result.requestID == nil, result.decision == nil else { throw MobileError.invalidResponse }
                return
            }
            guard result.fingerprint == nil, result.acceptedAtUnix == nil else { throw MobileError.invalidResponse }
            if pending.request.command.isPlan {
                guard let plan = result.planID, validID(plan), result.status == "proposed", result.todoIndex == nil, result.nodeID == nil,
                      result.runID == nil, result.sessionID == nil, result.turnID == nil, result.queueID == nil,
                      result.requestID == nil, result.decision == nil else { throw MobileError.invalidResponse }
                return
            }
            guard let node = result.nodeID, node > 0, result.planID == nil, result.todoIndex == nil else { throw MobileError.invalidResponse }
            if case let .decideApproval(target, decision) = pending.request.command {
                guard result.nodeID == target.nodeID, result.runID == target.runID, result.sessionID == target.sessionID,
                      result.turnID == target.turnID, result.requestID == target.requestID, result.decision == decision,
                      ["submitted", "request_closed"].contains(result.status ?? "") else { throw MobileError.invalidResponse }
            }
            if case let .deleteCard(card, _) = pending.request.command, result.nodeID != card.nodeID { throw MobileError.invalidResponse }
            if case let .interruptTurn(card) = pending.request.command {
                guard result.nodeID == card.nodeID, result.runID == card.runID,
                      result.sessionID == card.sessionID, result.turnID == card.turnID,
                      let status = result.status, ["interrupted", "completed", "failed"].contains(status) else { throw MobileError.invalidResponse }
            }
            if case let .queueMessage(card, _) = pending.request.command {
                guard result.nodeID == card.nodeID, result.runID == card.runID, result.sessionID == card.sessionID,
                      result.queueID == operationID, result.status == "queued", result.turnID == nil else { throw MobileError.invalidResponse }
            }
            if case let .sendMessage(card, _) = pending.request.command {
                guard result.nodeID == card.nodeID, let run = result.runID, let session = result.sessionID, let turn = result.turnID,
                      [run, session, turn].allSatisfy(validID), card.runID == nil || card.runID == run,
                      card.sessionID == nil || card.sessionID == session else { throw MobileError.invalidResponse }
            }
        case "rejected": guard result == nil, let error, validID(error), error != "outcome_unknown" else { throw MobileError.invalidResponse }
        case "outcome_unknown": guard result == nil, error == "outcome_unknown" else { throw MobileError.invalidResponse }
        default: throw MobileError.invalidResponse
        }
    }
}

public actor PendingCommandStore {
    private let url: URL
    public init(url: URL) { self.url = url }
    public func load() throws -> [PendingMobileCommand] {
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        let data = try Data(contentsOf: url)
        // The 256 KiB local budget includes full message bodies; it can fill before 128 records.
        guard data.count <= 256 * 1024 else { throw MobileError.invalidResponse }
        let commands = try JSONDecoder().decode([PendingMobileCommand].self, from: data)
        guard commands.count <= 128, Set(commands.map(\.id)).count == commands.count else { throw MobileError.invalidResponse }
        try commands.forEach { try $0.validate() }
        return commands
    }
    public func save(_ command: PendingMobileCommand) throws {
        try command.validate()
        var commands = try load()
        if let existing = commands.first(where: { $0.id == command.id }) {
            guard existing == command else { throw MobileError.invalidResponse }
            return
        }
        guard commands.count < 128 else { throw MobileError.invalidResponse }
        commands.append(command)
        try write(commands)
    }
    public func remove(id: String) throws { try write(load().filter { $0.id != id }) }
    private func write(_ commands: [PendingMobileCommand]) throws {
        let data = try JSONEncoder().encode(commands)
        // The write bound must equal load's 256 KiB bound, including newly stored message bodies.
        guard data.count <= 256 * 1024 else { throw MobileError.invalidResponse }
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        #if os(iOS)
        try data.write(to: url, options: [.atomic, .completeFileProtection])
        #else
        try data.write(to: url, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
        #endif
    }
}

extension MobileClient {
    public func prepareInterrupt(_ machine: PairedMachine, workspaceID: String, routeID: String, revision: UInt64, card: CardSnapshot) async throws -> PendingMobileCommand {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try PendingMobileCommand(machineID: machine.id, deviceID: credential.deviceID, workspaceID: workspaceID, routeID: routeID, revision: revision, interrupting: card)
    }
    public func prepareMessage(_ machine: PairedMachine, workspaceID: String, routeID: String, revision: UInt64, card: CardSnapshot, text: String, draftID: String? = nil) async throws -> PendingMobileCommand {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try PendingMobileCommand(machineID: machine.id, deviceID: credential.deviceID, workspaceID: workspaceID, routeID: routeID, revision: revision, sending: card, text: text, draftID: draftID)
    }
    public func prepareCardCreation(_ machine: PairedMachine, workspaceID: String, routeID: String, revision: UInt64, profile: String, title: String?) async throws -> PendingMobileCommand {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try PendingMobileCommand(machineID: machine.id, deviceID: credential.deviceID, workspaceID: workspaceID, routeID: routeID, revision: revision, profile: profile, title: title)
    }
    public func prepareCardDeletion(_ machine: PairedMachine, workspaceID: String, routeID: String, revision: UInt64, card: CardSnapshot, confirmBusy: Bool) async throws -> PendingMobileCommand {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try PendingMobileCommand(machineID: machine.id, deviceID: credential.deviceID, workspaceID: workspaceID, routeID: routeID, revision: revision, deleting: card, confirmBusy: confirmBusy)
    }
    /// The caller persists pending before invoking this exactly once; reconnect only queries receipt.
    public func submit(_ pending: PendingMobileCommand, to machine: PairedMachine) async throws -> MobileCommandReceipt {
        try await commandExchange(pending, machine: machine, submit: true)
    }
    public func receipt(for pending: PendingMobileCommand, on machine: PairedMachine) async throws -> MobileCommandReceipt {
        try await commandExchange(pending, machine: machine, submit: false)
    }
    private func commandExchange(_ pending: PendingMobileCommand, machine: PairedMachine, submit: Bool) async throws -> MobileCommandReceipt {
        try pending.validate(); try machine.validate()
        guard machine.id == pending.machineID, let credential = try await vault.load(machineID: machine.id), credential.deviceID == pending.deviceID else { throw MobileError.invalidCredential }
        var action: [String: Any] = ["type": submit ? "command" : "receipt", "device_id": credential.deviceID, "token": credential.token]
        if submit { action["request"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(pending.request)) }
        else { action["workspace_id"] = pending.request.workspaceID; action["operation_id"] = pending.request.operationID }
        let reply = try await exchange(machine, action: action)
        guard reply.result?.status == "receipt", let receipt = reply.result?.receipt else { throw MobileError.invalidResponse }
        try receipt.validate(for: pending)
        return receipt
    }
}
