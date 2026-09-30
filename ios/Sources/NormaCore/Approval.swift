import Foundation

public enum ApprovalDecision: String, Codable, Sendable { case allow, deny }

/// One physical drag; cancellation is latched until the next gesture begins.
public struct ApprovalSwipe: Equatable, Sendable {
    public private(set) var dx: Double = 0
    public private(set) var decision: ApprovalDecision?
    private var cancelled = false
    public init() {}
    public mutating func cancel() { cancelled = true; dx = 0; decision = nil }
    public mutating func update(dx: Double, dy: Double, width: Double) {
        guard !cancelled, dx.isFinite, dy.isFinite, width.isFinite, width > 0 else { cancel(); return }
        // Same intent lock as the prototype: vertical motion over 8pt wins at a 1.25 ratio.
        guard !(abs(dy) > 8 && abs(dy) > abs(dx) * 1.25) else { cancel(); return }
        self.dx = dx
        // 320 × 0.28 = 89.6pt; clamp to 88...118pt across phone widths.
        let threshold = min(118, max(88, width * 0.28))
        decision = abs(dx) >= threshold && abs(dx) > abs(dy) * 1.25 ? (dx > 0 ? .allow : .deny) : nil
    }
}

public struct ApprovalTarget: Codable, Sendable, Equatable {
    public let nodeID: UInt64
    public let profile: String
    public let runID: String
    public let sessionID: String
    public let turnID: String
    public let requestID: String
    enum CodingKeys: String, CodingKey {
        case nodeID = "node_id", profile, runID = "run_id", sessionID = "session_id", turnID = "turn_id", requestID = "request_id"
    }
    func validate() throws {
        guard nodeID > 0, ["Codex", "Claude"].contains(profile), [runID, sessionID, turnID].allSatisfy(validID),
              !requestID.isEmpty, requestID.utf8.count <= 256,
              requestID.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || [45, 95, 58].contains($0) }) else { throw MobileError.invalidResponse }
    }
    public func card(in project: ProjectSnapshot, workspaceID: String, routeID: String) throws -> CardSnapshot {
        guard project.id == workspaceID, project.state == "open", project.routeID == routeID,
              let card = project.cards?.first(where: { $0.id == nodeID }), card.profile == profile,
              card.runID == runID, card.sessionID == sessionID, card.turnID == turnID else { throw MobileError.sessionChanged }
        return card
    }
}

/// Preserve JSON integers through Decimal instead of rounding security-relevant parameters via Double.
public indirect enum ApprovalDetails: Codable, Sendable, Equatable {
    case object([String: ApprovalDetails]), array([ApprovalDetails]), text(String), number(Decimal), bool(Bool), null
    public init(from decoder: any Decoder) throws {
        let value = try decoder.singleValueContainer()
        if value.decodeNil() { self = .null }
        else if let bool = try? value.decode(Bool.self) { self = .bool(bool) }
        else if let text = try? value.decode(String.self) { self = .text(text) }
        else if let number = try? value.decode(Decimal.self) { self = .number(number) }
        else if let array = try? value.decode([Self].self) { self = .array(array) }
        else { self = .object(try value.decode([String: Self].self)) }
    }
    public func encode(to encoder: any Encoder) throws {
        var value = encoder.singleValueContainer()
        switch self {
        case let .object(object): try value.encode(object)
        case let .array(array): try value.encode(array)
        case let .text(text): try value.encode(text)
        case let .number(number): try value.encode(number)
        case let .bool(bool): try value.encode(bool)
        case .null: try value.encodeNil()
        }
    }
    public func formatted() throws -> String {
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        return String(decoding: try encoder.encode(self), as: UTF8.self)
    }
}

public struct ApprovalSnapshot: Codable, Sendable, Equatable {
    public let target: ApprovalTarget
    public let cardTitle: String
    public let tool: String
    public let details: ApprovalDetails
    public let decisions: [ApprovalDecision]
    public var requiresDesktop: Bool { ["external_permission_hook", "native_terminal"].contains(tool) }
    enum CodingKeys: String, CodingKey { case target, cardTitle = "card_title", tool, details, decisions }
    func validate() throws {
        try target.validate()
        // Allow 2 × Provider's 64KiB payload for native JSON encoding differences; never truncate.
        guard cardTitle.count <= 256, !tool.isEmpty, tool.count <= 256, (requiresDesktop ? decisions.isEmpty : !decisions.isEmpty),
              decisions.count <= 2, Set(decisions).count == decisions.count,
              try JSONEncoder().encode(details).count <= 128 * 1024 else { throw MobileError.invalidResponse }
    }
}

public struct ApprovalListRequest: Codable, Sendable, Equatable {
    public let workspaceID: String
    public let routeID: String
    public init(workspaceID: String, routeID: String) { self.workspaceID = workspaceID; self.routeID = routeID }
    enum CodingKeys: String, CodingKey { case workspaceID = "workspace_id", routeID = "route_id" }
}
struct ApprovalListResponse: Decodable, Sendable {
    let request: ApprovalListRequest
    let items: [ApprovalSnapshot]
}

extension PendingMobileCommand {
    public func matchesApproval(machineID: String, workspaceID: String, routeID: String, target: ApprovalTarget) -> Bool {
        guard case .decideApproval(let expected, _) = request.command else { return false }
        return self.machineID == machineID && request.workspaceID == workspaceID && request.routeID == routeID && expected == target
    }

    public init(machineID: String, deviceID: String, workspaceID: String, routeID: String, revision: UInt64, approving snapshot: ApprovalSnapshot, decision: ApprovalDecision, operationID: String = UUID().uuidString) throws {
        self.machineID = machineID; self.deviceID = deviceID; approvalPreview = snapshot
        request = Request(operationID: operationID, workspaceID: workspaceID, routeID: routeID, grantRevision: revision, command: .decideApproval(expected: snapshot.target, decision: decision))
        try validate()
    }
}

extension MobileClient {
    public func approvals(_ machine: PairedMachine, request: ApprovalListRequest) async throws -> [ApprovalSnapshot] {
        try machine.validate()
        guard validID(request.workspaceID), validID(request.routeID) else { throw MobileError.invalidResponse }
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        let reply = try await exchange(machine, action: ["type": "approvals", "device_id": credential.deviceID, "token": credential.token,
            "request": try JSONSerialization.jsonObject(with: JSONEncoder().encode(request))])
        guard reply.result?.status == "approvals", let list = reply.result?.approvals, list.request == request,
              list.items.count <= 128, Set(list.items.map { $0.target.runID + ":" + $0.target.requestID }).count == list.items.count else { throw MobileError.invalidResponse }
        try list.items.forEach { try $0.validate() }
        return list.items
    }
    public func prepareApproval(_ machine: PairedMachine, workspaceID: String, routeID: String, revision: UInt64, snapshot: ApprovalSnapshot, decision: ApprovalDecision) async throws -> PendingMobileCommand {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try PendingMobileCommand(machineID: machine.id, deviceID: credential.deviceID, workspaceID: workspaceID, routeID: routeID, revision: revision, approving: snapshot, decision: decision)
    }
}
