import Foundation

public struct DeliveryFeedback: Codable, Sendable, Equatable {
    public let key: MobileTurnOutcome.Key
    public let fingerprint: String
    public let text: String
    public struct Reference: Codable, Sendable, Equatable {
        public let key: MobileTurnOutcome.Key
        public let fingerprint: String
    }
    public var reference: Reference { Reference(key: key, fingerprint: fingerprint) }
    public func validate(for card: CardSnapshot) throws {
        try key.validate(); try card.validate()
        //32KiB Provider input -2KiB bounded origin context =30KiB original opinion.
        guard validHex(fingerprint), card.nodeID == key.nodeID, card.profile == key.profile,
              card.runID == key.runID, card.sessionID == key.sessionID, card.mobileControl,
              ["ready", "done", "ended", "failed"].contains(card.state),
              !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, text.utf8.count <= 30 * 1024,
              !text.unicodeScalars.contains(where: { ($0.value < 32 || (127...159).contains($0.value)) && $0.value != 9 && $0.value != 10 }) else { throw MobileError.invalidResponse }
    }
}

extension MobileTurnOutcome.Key {
    func validate() throws {
        guard nodeID > 0, nodeID <= Int64.max, MobileAgentProfile.supports(profile),
              [runID, sessionID, turnID].allSatisfy(validID) else { throw MobileError.invalidResponse }
    }
}
public struct TaskDeliveryReference: Decodable, Sendable {
    public let outcome: MobileTurnOutcome
    public let fingerprint: String
    public let savedAtUnix: UInt64
    public var acceptance: DeliveryAcceptance? = nil
    enum CodingKeys: String, CodingKey { case outcome, fingerprint, acceptance, savedAtUnix = "saved_at_unix" }
    func validate() throws {
        try outcome.key.validate()
        guard validHex(fingerprint), savedAtUnix > 0, savedAtUnix <= Int64.max else { throw MobileError.invalidResponse }
        if let acceptance {
            try acceptance.validate()
            guard outcome.state == .completed, acceptance.key == outcome.key, acceptance.fingerprint == fingerprint else { throw MobileError.invalidResponse }
        }
    }
}
public struct TaskDeliverySummary: Codable, Equatable, Sendable, Identifiable {
    public let sequence: UInt64
    public let workspaceID: String
    public let outcome: MobileTurnOutcome
    public let fingerprint: String
    public let savedAtUnix: UInt64
    public let status: String
    public let fileCount: Int
    public var acceptance: DeliveryAcceptance?
    public var id: String { fingerprint }
    public var available: Bool { status == "available" }
    public var stateLabel: String {
        switch outcome.state { case .completed: "执行完成"; case .failed: "执行失败"; case .interrupted: "已停止" }
    }
    public var explanation: String {
        switch status {
        case "available": "本轮项目变更已保存，后续任务不会改写这份记录。"
        case "baseline_missing", "baseline_unavailable": "本轮缺少可用的任务起点，无法提供可信的文件对比。"
        case "task_changed": "保存前任务状态已变化，无法确认这份成果。"
        case "window_closed": "项目窗口已关闭，未能保存成果。"
        case "capture_expired": "整理超时，本轮未保存文件内容。"
        case "capture_too_large": "成果超出预览大小上限，请在电脑上查看。"
        case "capture_changed": "整理时文件仍在变化，本轮未保存文件内容。"
        default: "本轮项目状态暂不支持文件对比，请在电脑上查看。"
        }
    }
    enum CodingKeys: String, CodingKey {
        case sequence, workspaceID = "workspace_id", outcome, fingerprint, status, acceptance
        case savedAtUnix = "saved_at_unix", fileCount = "file_count"
    }
    func validate(workspaceID: String, nodeID: UInt64?) throws {
        try TaskDeliveryReference(outcome: outcome, fingerprint: fingerprint, savedAtUnix: savedAtUnix, acceptance: acceptance).validate()
        if let acceptance, !available || acceptance.workspaceID != workspaceID { throw MobileError.invalidResponse }
        guard self.workspaceID == workspaceID, nodeID.map({ outcome.key.nodeID == $0 }) ?? true, sequence > 0, sequence <= Int64.max,
              (0...128).contains(fileCount), available || fileCount == 0,
              ["available", "baseline_missing", "baseline_unavailable", "capture_expired", "capture_too_large", "capture_unsafe_path", "capture_changed", "capture_unavailable", "capture_unsupported", "task_changed", "window_closed"].contains(status) else { throw MobileError.invalidResponse }
    }
}
public struct TaskDeliveryPage: Decodable, Sendable {
    public let request: ProjectReviewRequest
    public let nodeID: UInt64?
    public let items: [TaskDeliverySummary]
    public let nextCursor: UInt64?
    public let pendingOnly: Bool?
    public let pendingCount: UInt64?
    public let collecting: Bool?
    enum CodingKeys: String, CodingKey {
        case request, nodeID = "node_id", items, nextCursor = "next_cursor"
        case pendingOnly = "pending_only", pendingCount = "pending_count", collecting
    }
    func validate(for scope: ProjectReviewRequest, nodeID: UInt64?, before: UInt64?, pendingOnly: Bool = false) throws {
        try scope.validate()
        guard request == scope, scope.deliveryKey == nil, self.nodeID == nodeID, nodeID.map({ $0 > 0 && $0 <= Int64.max }) ?? true,
              before.map({ $0 > 0 && $0 <= Int64.max }) ?? true, items.count <= 20,
              Set(items.map(\.fingerprint)).count == items.count else { throw MobileError.invalidResponse }
        guard self.pendingOnly == nil ? !pendingOnly : self.pendingOnly == pendingOnly,
              pendingCount.map({ $0 <= Int64.max }) ?? true else { throw MobileError.invalidResponse }
        if pendingOnly {
            guard let pendingCount, pendingCount >= items.count,
                  items.allSatisfy({ $0.available && $0.outcome.state == .completed && $0.acceptance == nil }) else {
                throw MobileError.invalidResponse
            }
        }
        var previous = before ?? UInt64.max
        for item in items {
            try item.validate(workspaceID: scope.workspaceID, nodeID: nodeID)
            guard item.sequence < previous else { throw MobileError.invalidResponse }
            previous = item.sequence
        }
        if let nextCursor {
            guard items.count == 20, nextCursor == items.last?.sequence else { throw MobileError.invalidResponse }
        }
    }
}
extension MobileClient {
    public func taskDeliveries(_ machine: PairedMachine, scope: ProjectReviewRequest, nodeID: UInt64? = nil, before: UInt64? = nil) async throws -> TaskDeliveryPage {
        try machine.validate(); try scope.validate()
        guard scope.deliveryKey == nil, nodeID.map({ $0 > 0 && $0 <= Int64.max }) ?? true,
              before.map({ $0 > 0 && $0 <= Int64.max }) ?? true else { throw MobileError.invalidResponse }
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        var action: [String: Any] = ["type": "task_deliveries", "device_id": credential.deviceID, "token": credential.token,
            "request": try JSONSerialization.jsonObject(with: JSONEncoder().encode(scope))]
        if let nodeID { action["node_id"] = nodeID }
        if let before { action["before"] = before }
        //Two2s route checks + SQLite/scheduling6s =10s; retries remain explicit.
        let reply = try await exchange(machine, action: action, timeout: .seconds(10))
        guard reply.result?.status == "task_deliveries", let page = reply.result?.deliveries else { throw MobileError.invalidResponse }
        try page.validate(for: scope, nodeID: nodeID, before: before)
        return page
    }
}

public struct DeliveryAcceptance: Codable, Equatable, Sendable {
    public let workspaceID: String
    public let routeID: String
    public let key: MobileTurnOutcome.Key
    public let fingerprint: String
    public let deviceID: String
    public let operationID: String
    public let acceptedAtUnix: UInt64
    enum CodingKeys: String, CodingKey {
        case workspaceID = "workspace_id", routeID = "route_id", key, fingerprint
        case deviceID = "device_id", operationID = "operation_id", acceptedAtUnix = "accepted_at_unix"
    }
    func validate() throws {
        try key.validate()
        guard [workspaceID, routeID, deviceID, operationID].allSatisfy(validID), validHex(fingerprint),
              acceptedAtUnix > 0, acceptedAtUnix <= Int64.max else { throw MobileError.invalidResponse }
    }
}
extension PendingMobileCommand {
    public init(machineID: String, deviceID: String, workspaceID: String, routeID: String, revision: UInt64,
                feedbackFor delivery: TaskDeliveryReference, card: CardSnapshot, text: String, draftID: String? = nil,
                operationID: String = UUID().uuidString) throws {
        try delivery.validate()
        self.machineID = machineID; self.deviceID = deviceID; self.draftID = draftID
        request = Request(operationID: operationID, workspaceID: workspaceID, routeID: routeID, grantRevision: revision,
                          command: .feedbackDelivery(expected: card, feedback: DeliveryFeedback(key: delivery.outcome.key, fingerprint: delivery.fingerprint, text: text)))
        try validate()
    }
    public init(machineID: String, deviceID: String, workspaceID: String, routeID: String, revision: UInt64,
                accepting delivery: TaskDeliveryReference, operationID: String = UUID().uuidString) throws {
        try delivery.validate()
        guard delivery.outcome.state == .completed, delivery.acceptance == nil else { throw MobileError.invalidResponse }
        self.machineID = machineID; self.deviceID = deviceID
        request = Request(operationID: operationID, workspaceID: workspaceID, routeID: routeID, grantRevision: revision,
                          command: .acceptDelivery(key: delivery.outcome.key, fingerprint: delivery.fingerprint))
        try validate()
    }
}
extension MobileClient {
    public func prepareDeliveryFeedback(_ machine: PairedMachine, workspaceID: String, routeID: String, revision: UInt64,
                                        delivery: TaskDeliveryReference, card: CardSnapshot, text: String, draftID: String? = nil) async throws -> PendingMobileCommand {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try PendingMobileCommand(machineID: machine.id, deviceID: credential.deviceID, workspaceID: workspaceID,
                                        routeID: routeID, revision: revision, feedbackFor: delivery, card: card, text: text, draftID: draftID)
    }
    public func prepareDeliveryAcceptance(_ machine: PairedMachine, workspaceID: String, routeID: String, revision: UInt64,
                                          delivery: TaskDeliveryReference) async throws -> PendingMobileCommand {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try PendingMobileCommand(machineID: machine.id, deviceID: credential.deviceID, workspaceID: workspaceID,
                                        routeID: routeID, revision: revision, accepting: delivery)
    }
}
