import Foundation

public struct ThreadAcceptanceDecision: Codable, Equatable, Sendable {
    let type: String
    public let operationID: String
    public let target: Target
    public struct Target: Codable, Equatable, Sendable {
        let collaborationWorkspaceID: String
        let threadID: String
        let operationID: String
        let taskRef: String
        let fingerprint: String
        enum CodingKeys: String, CodingKey {
            case collaborationWorkspaceID = "collaboration_workspace_id", threadID = "thread_id",
                 operationID = "operation_id", taskRef = "task_ref", fingerprint
        }
    }
    enum CodingKeys: String, CodingKey { case type, operationID = "operation_id", target }

    public init(operation: ThreadTaskOutboxOperation, operationID: String = UUID().uuidString) throws {
        guard let device = operation.deviceID, let receipt = operation.receipt,
              let taskRef = receipt.receipt.result?.taskRef,
              let delivery = receipt.execution?.execution.delivery, delivery.available,
              delivery.outcome.state == .completed else { throw MobileError.reviewUnavailable }
        try receipt.validate(task: operation.task, machineID: operation.machineID, deviceID: device)
        type = "accept_delivery"; self.operationID = operationID
        target = Target(collaborationWorkspaceID: operation.task.collaborationWorkspaceID, threadID: operation.task.threadID,
                        operationID: operation.id, taskRef: taskRef, fingerprint: delivery.fingerprint)
        guard UUID(uuidString: operationID) != nil, operationID != operation.id,
              operationID != operation.task.sourceOperationID else { throw MobileError.invalidResponse }
    }
}

public struct ThreadAcceptanceState: Codable, Equatable, Sendable {
    public let decision: ThreadAcceptanceDecision
    public var receipt: MobileCommandReceipt?
    public var acceptance: DeliveryAcceptance?
    public var awaitingReceipt: Bool {
        acceptance == nil && (receipt == nil || receipt?.state == "accepted" || receipt?.state == "succeeded")
    }
    public var statusLabel: String {
        if acceptance != nil { return "已验收" }
        return switch receipt?.state {
        case "rejected": "验收未保存"
        case "outcome_unknown": "验收结果待核对"
        default: "验收待确认"
        }
    }
    public var explanation: String {
        if acceptance != nil { return "验收记录已保存在 Mac，本轮成果将继续保留。" }
        return switch receipt?.error {
        case "state_not_ready": "Agent 正在执行另一项任务，请稍后重新验收。"
        case "delivery_not_ready": "本轮成果尚未完成或存在冲突，请在 Mac 上核对。"
        case "route_expired": "Mac 的项目已关闭，请打开项目后重新验收。"
        case "unauthorized": "当前手机已无权验收此项目，请检查 Mac 授权。"
        case "outcome_unknown": "暂时无法确认 Mac 是否保存了验收，原操作已保留，请在 Mac 核对。"
        case .some: "Mac 未确认这次验收，请检查连接与项目状态。"
        case .none: "正在等待 Mac 保存验收记录。恢复连接后会继续核对。"
        }
    }

    func validate(for operation: ThreadTaskOutboxOperation) throws {
        let expected = try ThreadAcceptanceDecision(operation: operation, operationID: decision.operationID)
        guard decision == expected, let device = operation.deviceID,
              let delivery = operation.receipt?.execution?.execution.delivery else { throw MobileError.invalidResponse }
        if let receipt {
            // Grant revision is irrelevant to an already-authenticated stored receipt; 1 is the valid lower bound.
            let pending = try PendingMobileCommand(machineID: operation.machineID, deviceID: device,
                workspaceID: operation.task.projectID, routeID: receipt.routeID, revision: 1,
                accepting: TaskDeliveryReference(outcome: delivery.outcome, fingerprint: delivery.fingerprint, savedAtUnix: delivery.savedAtUnix),
                operationID: decision.operationID)
            try receipt.validate(for: pending)
        }
        if let acceptance {
            try acceptance.validate()
            guard acceptance.workspaceID == operation.task.projectID, acceptance.key == delivery.outcome.key,
                  acceptance.fingerprint == delivery.fingerprint else { throw MobileError.invalidResponse }
            if receipt?.state == "succeeded" {
                guard acceptance.operationID == decision.operationID, acceptance.deviceID == device,
                      acceptance.routeID == receipt?.routeID,
                      acceptance.acceptedAtUnix == receipt?.result?.acceptedAtUnix else { throw MobileError.invalidResponse }
            }
        }
    }
}

extension CollaborationRelayConnection {
    public func threadTaskAcceptance(for operation: ThreadTaskOutboxOperation) async throws -> ThreadAcceptanceState {
        guard capabilities.contains("thread_task_acceptance_v1") else { throw ThreadTaskError.acceptanceRequiresUpgrade }
        guard let grant, let state = operation.acceptance, operation.machineID == machineID,
              operation.deviceID == grant.deviceID else { throw MobileError.invalidCredential }
        try state.validate(for: operation)
        let fields: [String: Any] = ["decision": try JSONSerialization.jsonObject(with: JSONEncoder().encode(state.decision))]
        do {
            let reply = try await exchange(type: "thread_task_acceptance", fields: fields)
            guard reply.result?.status == "thread_task_acceptance", let result = reply.result?.threadAcceptance,
                  result.decision == state.decision else { throw MobileError.invalidResponse }
            try result.validate(for: operation)
            return result
        } catch { close(); throw error }
    }
}
