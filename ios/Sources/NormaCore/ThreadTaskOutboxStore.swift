import Foundation
import SQLite3

public struct ThreadTaskOutboxOperation: Codable, Equatable, Identifiable, Sendable {
    public var id: String { task.operationID }
    public let machineID: String
    public let task: ThreadTaskOperation
    public var target: WorkspaceAgentReference? = nil
    public var projectName: String? = nil
    public internal(set) var deviceID: String?
    public internal(set) var receipt: ThreadTaskReceipt?
    public internal(set) var replyMessageID: String?
    public internal(set) var acceptance: ThreadAcceptanceState?
    public internal(set) var reviewDraft: WorkThreadDraft?
    public internal(set) var revisionOperationID: String?
    public internal(set) var progress: [ThreadTaskProgress]? = nil
    public internal(set) var terminalOperation: ThreadTerminalState? = nil
    public internal(set) var terminalDraft: WorkThreadDraft? = nil
    public var awaitsDispatchReceipt: Bool { receipt == nil || receipt?.receipt.state == "accepted" }
    public var awaitsExecutionReceipt: Bool {
        guard receipt?.receipt.state == "succeeded" else { return false }
        guard let execution = receipt?.execution?.execution else { return true }
        return !execution.terminal || execution.delivery == nil
    }
}
