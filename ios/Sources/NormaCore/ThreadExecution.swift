import Foundation

/// Actual receipt observations on this phone, not inferred Mac execution timestamps or model steps.
public struct ThreadTaskProgress: Codable, Equatable, Identifiable, Sendable {
    public var id: String { "\(dispatchState):\(executionSequence ?? 0)" }
    public let dispatchState: String
    public let executionState: String?
    public let executionSequence: UInt64?
    public let error: String?
    public let observedAt: Date

    init(_ receipt: ThreadTaskReceipt, at date: Date) {
        dispatchState = receipt.receipt.state
        executionState = receipt.execution?.execution.state
        executionSequence = receipt.execution?.sequence
        error = receipt.execution?.execution.error ?? receipt.receipt.error
        observedAt = date
    }

    func sameState(as other: Self) -> Bool {
        dispatchState == other.dispatchState && executionState == other.executionState && error == other.error
    }

    func validate(through receipt: ThreadTaskReceipt) throws {
        guard ["accepted", "succeeded", "rejected", "outcome_unknown"].contains(dispatchState),
              observedAt.timeIntervalSince1970.isFinite, observedAt.timeIntervalSince1970 > 0,
              (error?.utf8.count ?? 0) <= 4096,
              (executionState == nil) == (executionSequence == nil) else { throw WorkThreadError.damagedStore }
        if let state = executionState, let sequence = executionSequence {
            guard dispatchState == "succeeded", sequence > 0, sequence <= (receipt.execution?.sequence ?? 0),
                  ["accepted", "submitted", "queued", "untracked", "done", "failed", "cancel_requested", "cancelled", "interrupted"].contains(state) else {
                throw WorkThreadError.damagedStore
            }
        }
    }
}

public struct ThreadExecutionEvent: Codable, Equatable, Sendable {
    public let sequence: UInt64
    public let eventID: String
    public let execution: ThreadExecution
    enum CodingKeys: String, CodingKey { case sequence, eventID = "event_id", execution }

    func validate(task: ThreadTaskOperation, machineID: String, receipt: ThreadTaskReceipt.Receipt) throws {
        guard sequence > 0, sequence <= Int64.max, eventID == "\(machineID):execution:\(sequence)",
              receipt.state == "succeeded", execution.taskRef == receipt.result?.taskRef,
              execution.nodeID == receipt.result?.nodeID, execution.agentDefinitionID == task.agentDefinitionID,
              ["accepted", "submitted", "queued", "untracked", "done", "failed", "cancel_requested", "cancelled", "interrupted"].contains(execution.state),
              ["managed", "untracked"].contains(execution.tracking),
              execution.result == nil || execution.terminal,
              (execution.result?.text?.utf8.count ?? 0) <= 16 * 1024,
              (execution.error?.utf8.count ?? 0) <= 4096,
              // Same 112KiB projection limit as Mac; escaped 16KiB text can take 6x its original bytes.
              try JSONEncoder().encode(execution).count <= 112 * 1024 else { throw ThreadTaskError.invalidReceipt }
        try execution.delivery?.validate(workspaceID: task.projectID, nodeID: execution.nodeID)
    }

    func merging(_ previous: Self?) throws -> Self {
        guard let previous else { return self }
        if sequence < previous.sequence { return previous }
        if sequence == previous.sequence {
            guard self == previous else { throw ThreadTaskError.invalidReceipt }
            return previous
        }
        if previous.execution.terminal {
            var withDelivery = previous.execution
            withDelivery.delivery = execution.delivery
            guard withDelivery == execution else { throw ThreadTaskError.invalidReceipt }
        }
        if let delivery = previous.execution.delivery, execution.delivery != delivery {
            throw ThreadTaskError.invalidReceipt
        }
        return self
    }
}

public struct ThreadExecution: Codable, Equatable, Sendable {
    public let taskRef: String
    public let nodeID: UInt64
    public let agentDefinitionID: Int64
    public let state: String
    public let tracking: String
    public let error: String?
    public let result: Result?
    public var delivery: TaskDeliverySummary?
    public var terminal: Bool { ["done", "failed", "cancelled", "interrupted"].contains(state) }
    public struct Result: Codable, Equatable, Sendable {
        public let text: String?
        public let truncated: Bool
    }
    enum CodingKeys: String, CodingKey {
        case taskRef = "task_ref", nodeID = "node_id", agentDefinitionID = "agent_definition_id", state, tracking, error, result, delivery
    }
}
