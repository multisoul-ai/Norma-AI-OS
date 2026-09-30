import Foundation

public struct ThreadPlanAgent: Codable, Equatable, Sendable {
    public let id: String
    public let name: String
    public let project: String
    public let profile: String
}

public struct ThreadPlanRequest: Codable, Equatable, Sendable {
    public let turnID: String
    public let workspaceID: String
    public let threadID: String
    public let messageID: String
    public let message: String
    public let context: String
    public let agents: [ThreadPlanAgent]
    enum CodingKeys: String, CodingKey {
        case turnID = "turn_id", workspaceID = "workspace_id", threadID = "thread_id", messageID = "message_id"
        case message, context, agents
    }

    func validate() throws {
        guard [turnID, workspaceID, threadID, messageID].allSatisfy({ UUID(uuidString: $0) != nil }),
              turnID != messageID, planText(message, bytes: 8192), planText(context, bytes: 32768),
              agents.count <= 64, Set(agents.map(\.id)).count == agents.count,
              agents.allSatisfy({ [$0.id, $0.name, $0.project, $0.profile].allSatisfy { planText($0, bytes: 4096) } }),
              let scope = try? JSONSerialization.jsonObject(with: Data(context.utf8)) as? [String: Any],
              scope["workspaceID"] as? String == workspaceID, scope["threadID"] as? String == threadID,
              try JSONEncoder().encode(self).count <= 65536 else { throw CloudTextError.invalidRequest }
        _ = try allowedTargets()
    }

    func allowedTargets() throws -> Set<String> {
        guard let scope = try? JSONSerialization.jsonObject(with: Data(context.utf8)) as? [String: Any],
              scope["addressedAgentIDs"] == nil || scope["addressedAgentIDs"] is [String] else {
            throw CloudTextError.invalidRequest
        }
        let selected = scope["addressedAgentIDs"] as? [String] ?? []
        let members = Set(agents.map(\.id))
        guard Set(selected).count == selected.count, Set(selected).isSubset(of: members) else {
            throw CloudTextError.invalidRequest
        }
        func name(_ agent: ThreadPlanAgent) -> String { agent.name.precomposedStringWithCanonicalMapping.lowercased() }
        let selectedNames = Set(agents.filter { selected.contains($0.id) }.map(name))
        return Set(agents.filter { selected.contains($0.id) || !selectedNames.contains(name($0)) }.map(\.id))
    }
}

public struct ThreadPlanAssignment: Codable, Equatable, Sendable {
    public let targetID: String
    public let task: String
    enum CodingKeys: String, CodingKey { case targetID = "target_id", task }
}

public struct ThreadPlanResponse: Codable, Equatable, Sendable {
    public let turnID: String
    public let workspaceID: String
    public let threadID: String
    public let messageID: String
    public let reply: String
    public let assignments: [ThreadPlanAssignment]
    enum CodingKeys: String, CodingKey {
        case turnID = "turn_id", workspaceID = "workspace_id", threadID = "thread_id", messageID = "message_id"
        case reply, assignments
    }

    func validate(for request: ThreadPlanRequest) throws {
        struct Candidate: Encodable { let reply: String; let assignments: [ThreadPlanAssignment] }
        let targets = try request.allowedTargets()
        guard turnID == request.turnID, workspaceID == request.workspaceID,
              threadID == request.threadID, messageID == request.messageID,
              planText(reply, bytes: 8192), assignments.count <= 8,
              Set(assignments.map(\.targetID)).count == assignments.count,
              assignments.allSatisfy({ targets.contains($0.targetID) && planText($0.task, bytes: 8192)
                && $0.task.utf8.count + request.context.utf8.count <= 32768 }),
              try JSONEncoder().encode(Candidate(reply: reply, assignments: assignments)).count <= 32768 else {
            throw CloudTextError.invalidResponse
        }
    }

    static func decode(_ data: Data, for request: ThreadPlanRequest) throws -> Self {
        // Codable alone ignores extra keys; reject execution/source extensions at the network boundary.
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys) == Set(["turn_id", "workspace_id", "thread_id", "message_id", "reply", "assignments"]),
              let assignments = object["assignments"] as? [[String: Any]],
              assignments.allSatisfy({ Set($0.keys) == Set(["target_id", "task"]) }),
              let result = try? JSONDecoder().decode(Self.self, from: data) else { throw CloudTextError.invalidResponse }
        try result.validate(for: request)
        return result
    }
}

private func planText(_ value: String, bytes: Int) -> Bool {
    !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && value.utf8.count <= bytes &&
    value.unicodeScalars.allSatisfy { $0.value == 9 || $0.value == 10 || ($0.value >= 32 && !(127...159).contains($0.value)) }
}

public struct PreparedThreadPlan: Codable, Equatable, Sendable {
    public let request: ThreadPlanRequest
    public let contextVersion: UInt64
    public let targets: [WorkspaceAgentReference]
    public let draftRevision: String?
}

struct ThreadPlanRecord: Codable, Identifiable {
    var id: String { prepared.request.turnID }
    let prepared: PreparedThreadPlan
    let response: ThreadPlanResponse
    let taskIDs: [String]
}
