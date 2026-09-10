import Foundation

public struct PlanPriority: Codable, Sendable, Equatable {
    public let planID: String
    public let todoIndex: Int
    public init(planID: String, todoIndex: Int) { self.planID = planID; self.todoIndex = todoIndex }
    enum CodingKeys: String, CodingKey { case planID = "plan_id", todoIndex = "todo_index" }
    func validate() throws {
        guard validID(planID), (0..<8).contains(todoIndex) else { throw MobileError.invalidResponse }
    }
}

public struct ProjectPlan: Codable, Sendable, Identifiable, Equatable {
    public let sequence: UInt64
    public let planID: String
    public let workspaceID: String
    public let routeID: String
    public let direction: String
    public let draft: Draft
    public let createdAtUnix: UInt64
    public var id: String { planID }
    public struct Draft: Codable, Sendable, Equatable {
        public let summary: String
        public let todos: [Todo]
    }
    public struct Todo: Codable, Sendable, Equatable {
        public let target: CardSnapshot
        public let title: String
        public let instruction: String
    }
    enum CodingKeys: String, CodingKey {
        case sequence, planID = "plan_id", workspaceID = "workspace_id", routeID = "route_id"
        case direction, draft, createdAtUnix = "created_at_unix"
    }
    func validate(workspaceID: String) throws {
        guard self.workspaceID == workspaceID, [planID, workspaceID, routeID].allSatisfy(validID),
              sequence > 0, sequence <= Int64.max, createdAtUnix > 0, createdAtUnix <= Int64.max,
              validPlanText(direction, maxBytes: 8 * 1024), validPlanText(draft.summary, maxBytes: 4096),
              draft.todos.count <= 8, try JSONEncoder().encode(draft).count <= 64 * 1024,
              try JSONEncoder().encode(self).count <= 80 * 1024 else { throw MobileError.invalidResponse }
        for todo in draft.todos {
            try todo.target.validate()
            guard todo.target.mobileControl, ["Codex", "Claude"].contains(todo.target.profile),
                  todo.target.sessionID == nil || todo.target.runID != nil,
                  validPlanText(todo.title, maxBytes: 320), validPlanText(todo.instruction, maxBytes: 8 * 1024) else { throw MobileError.invalidResponse }
        }
    }
}

public struct ProjectPlanPage: Decodable, Sendable {
    public let items: [ProjectPlan]
    public let nextCursor: UInt64?
    enum CodingKeys: String, CodingKey { case items, nextCursor = "next_cursor" }
    func validate(workspaceID: String, before: UInt64?) throws {
        guard validID(workspaceID), before.map({ $0 > 0 && $0 <= Int64.max }) ?? true,
              items.count <= 4, Set(items.map(\.id)).count == items.count else { throw MobileError.invalidResponse }
        var previous = before ?? UInt64.max
        for plan in items {
            try plan.validate(workspaceID: workspaceID)
            guard plan.sequence < previous else { throw MobileError.invalidResponse }
            previous = plan.sequence
        }
        if let nextCursor {
            guard items.count == 4, nextCursor == items.last?.sequence else { throw MobileError.invalidResponse }
        }
    }
}

extension PendingMobileCommand {
    public init(machineID: String, deviceID: String, workspaceID: String, routeID: String, revision: UInt64, planning direction: String, priority: PlanPriority? = nil, operationID: String = UUID().uuidString) throws {
        self.machineID = machineID; self.deviceID = deviceID
        request = Request(operationID: operationID, workspaceID: workspaceID, routeID: routeID, grantRevision: revision, command: .generatePlan(direction: direction, priority: priority))
        try validate()
    }
}

extension MobileClient {
    public func preparePlan(_ machine: PairedMachine, workspaceID: String, routeID: String, revision: UInt64, direction: String, priority: PlanPriority? = nil) async throws -> PendingMobileCommand {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try PendingMobileCommand(machineID: machine.id, deviceID: credential.deviceID, workspaceID: workspaceID, routeID: routeID, revision: revision, planning: direction, priority: priority)
    }
    public func projectPlans(_ machine: PairedMachine, workspaceID: String, before: UInt64? = nil) async throws -> ProjectPlanPage {
        try machine.validate()
        guard validID(workspaceID), before.map({ $0 > 0 && $0 <= Int64.max }) ?? true else { throw MobileError.invalidResponse }
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        var action: [String: Any] = ["type": "project_plans", "device_id": credential.deviceID, "token": credential.token, "workspace_id": workspaceID]
        if let before { action["before"] = before }
        let reply = try await exchange(machine, action: action)
        guard reply.result?.status == "project_plans", reply.result?.workspaceID == workspaceID,
              let plans = reply.result?.plans else { throw MobileError.invalidResponse }
        try plans.validate(workspaceID: workspaceID, before: before)
        return plans
    }
}

func validPlanText(_ text: String, maxBytes: Int) -> Bool {
    !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && text.utf8.count <= maxBytes &&
    !text.unicodeScalars.contains { ($0.value < 32 || (127...159).contains($0.value)) && $0.value != 9 && $0.value != 10 }
}
