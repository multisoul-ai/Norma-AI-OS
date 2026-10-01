import Foundation
import CryptoKit

/// A continuation of the original Agent session, with its own immutable source and operation.
public struct ThreadFollowupOperation: Codable, Equatable, Sendable {
    public let operationID: String
    public let collaborationWorkspaceID: String
    public let threadID: String
    public let sourceOperationID: String
    public let sourceMessageID: String
    public let parentOperationID: String
    public let taskRef: String
    public let text: String
    enum CodingKeys: String, CodingKey {
        case operationID = "operation_id", collaborationWorkspaceID = "collaboration_workspace_id", threadID = "thread_id",
             sourceOperationID = "source_operation_id", sourceMessageID = "source_message_id",
             parentOperationID = "parent_operation_id", taskRef = "task_ref", text
    }

    public init(operationID: String = UUID().uuidString, parent: ThreadTaskOutboxOperation,
                source: ThreadMessageOperation) throws {
        try source.validate(machineID: parent.machineID)
        guard let receipt = parent.receipt, receipt.receipt.state == "succeeded",
              let reference = receipt.receipt.result?.taskRef,
              source.collaborationWorkspaceID == parent.task.collaborationWorkspaceID,
              source.threadID == parent.task.threadID, source.projectIDs.contains(parent.task.projectID),
              source.operationID != parent.task.sourceOperationID, source.messageID != parent.task.sourceMessageID
        else { throw ThreadTaskError.invalidTask }
        self.operationID = operationID; collaborationWorkspaceID = source.collaborationWorkspaceID
        threadID = source.threadID; sourceOperationID = source.operationID; sourceMessageID = source.messageID
        parentOperationID = parent.id; taskRef = reference; text = source.text
        _ = try fingerprint(machineID: parent.machineID)
    }

    public func fingerprint(machineID: String) throws -> String {
        guard machineID.hasPrefix("mac-"), validHex(String(machineID.dropFirst(4))),
              [operationID, collaborationWorkspaceID, threadID, sourceOperationID, sourceMessageID, parentOperationID]
                .allSatisfy({ UUID(uuidString: $0) != nil }),
              operationID != sourceOperationID, operationID != parentOperationID,
              !taskRef.isEmpty, taskRef.utf8.count <= 160,
              !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, text.utf8.count <= 32 * 1024,
              text.unicodeScalars.allSatisfy({ $0.value == 9 || $0.value == 10 || ($0.value >= 32 && !(127...159).contains($0.value)) }),
              try JSONEncoder().encode(self).count <= 64 * 1024 else { throw ThreadTaskError.invalidTask }
        var bytes = Data("Norma thread followup v1\0".utf8)
        for value in [machineID, operationID, collaborationWorkspaceID, threadID, sourceOperationID,
                      sourceMessageID, parentOperationID, taskRef, text] {
            let data = Data(value.utf8); var length = UInt32(data.count).bigEndian
            withUnsafeBytes(of: &length) { bytes.append(contentsOf: $0) }; bytes.append(data)
        }
        return SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
    }

    func validate(parent: ThreadTaskOutboxOperation, originalKey: MobileTurnOutcome.Key? = nil) throws {
        _ = try fingerprint(machineID: parent.machineID)
        try originalKey?.validate()
        guard let receipt = parent.receipt, let device = parent.deviceID,
              parentOperationID == parent.id, collaborationWorkspaceID == parent.task.collaborationWorkspaceID,
              threadID == parent.task.threadID, receipt.receipt.state == "succeeded",
              taskRef == receipt.receipt.result?.taskRef,
              sourceOperationID != parent.task.sourceOperationID, sourceMessageID != parent.task.sourceMessageID
        else { throw ThreadTaskError.invalidTask }
        try receipt.validate(task: parent.task, machineID: parent.machineID, deviceID: device)
        if let originalKey, originalKey.nodeID != receipt.receipt.result?.nodeID
            || receipt.execution?.execution.delivery.map({ $0.outcome.key != originalKey }) == true {
            throw MobileError.sessionChanged
        }
    }
}

public struct ThreadFollowupReceipt: Codable, Equatable, Sendable {
    public let binding: ThreadTaskReceipt.Binding
    public let fingerprint: String
    public let receipt: MobileCommandReceipt
    public let originalKey: MobileTurnOutcome.Key
    public internal(set) var queuedMessage: QueuedMessageStatus
    public internal(set) var outcome: Outcome?
    public internal(set) var delivery: TaskDeliverySummary?
    public struct Outcome: Codable, Equatable, Sendable {
        public let key: MobileTurnOutcome.Key
        public let state: MobileTurnOutcome.State
        public let summary: String?
    }
    enum CodingKeys: String, CodingKey { case binding, fingerprint, receipt, originalKey = "original_key", queuedMessage = "queued_message", outcome, delivery }

    public func validate(operation: ThreadFollowupOperation, parent: ThreadTaskOutboxOperation,
                         originalKey expectedKey: MobileTurnOutcome.Key? = nil) throws {
        guard expectedKey == nil || expectedKey == originalKey else { throw ThreadTaskError.invalidReceipt }
        try operation.validate(parent: parent, originalKey: originalKey)
        guard let device = parent.deviceID, let original = parent.receipt,
              binding.sourceMacID == parent.machineID, binding.collaborationWorkspaceID == operation.collaborationWorkspaceID,
              binding.threadID == operation.threadID, binding.sourceOperationID == operation.sourceOperationID,
              binding.sourceMessageID == operation.sourceMessageID, binding.payload.utf8.count <= 64 * 1024,
              try JSONDecoder().decode(ThreadFollowupOperation.self, from: Data(binding.payload.utf8)) == operation,
              fingerprint == (try operation.fingerprint(machineID: parent.machineID)), receipt.state == "succeeded",
              // Same 256KiB relay limit minus16KiB outer routing/encryption margin as the Mac.
              try JSONEncoder().encode(self).count <= 240 * 1024 else { throw ThreadTaskError.invalidReceipt }
        // Reuse the existing queue's identity/state validators. Revision=1 is only a nonzero
        // validation placeholder; this value is never sent or used to authorize an operation.
        let card = CardSnapshot(nodeID: originalKey.nodeID, profile: originalKey.profile, title: "",
            mobileControl: true, state: "ready", runID: originalKey.runID, sessionID: originalKey.sessionID,
            taskTitle: nil, turnID: originalKey.turnID)
        let pending = try PendingMobileCommand(machineID: parent.machineID, deviceID: device,
            workspaceID: parent.task.projectID, routeID: original.receipt.routeID, revision: 1,
            queuing: card, text: operation.text, operationID: operation.operationID)
        try receipt.validate(for: pending)
        try queuedMessage.validate(for: pending)
        guard queuedMessage.result?.key == nil, queuedMessage.result?.taskRef == nil,
              queuedMessage.result?.feedback == nil, queuedMessage.result?.fingerprint == nil,
              queuedMessage.result?.acceptedAtUnix == nil, queuedMessage.result?.queueID == nil,
              queuedMessage.result?.requestID == nil, queuedMessage.result?.decision == nil,
              queuedMessage.result?.status == nil else { throw ThreadTaskError.invalidReceipt }
        if let outcome {
            try outcome.key.validate()
            guard queuedMessage.state == "sent", outcome.key.nodeID == originalKey.nodeID,
                  outcome.key.profile == originalKey.profile, outcome.key.runID == originalKey.runID,
                  outcome.key.sessionID == originalKey.sessionID, outcome.key.turnID == queuedMessage.result?.turnID,
                  // Reply previews follow the same16KiB display budget as thread execution results.
                  (outcome.summary?.utf8.count ?? 0) <= 16 * 1024 else { throw ThreadTaskError.invalidReceipt }
        }
        if let delivery {
            try delivery.validate(workspaceID: parent.task.projectID, nodeID: originalKey.nodeID)
            guard let outcome, delivery.outcome.key == outcome.key, delivery.outcome.state == outcome.state else {
                throw ThreadTaskError.invalidReceipt
            }
        }
    }
}

extension CollaborationRelayConnection {
    public func submitThreadFollowup(_ operation: ThreadFollowupOperation, parent: ThreadTaskOutboxOperation,
        originalKey: MobileTurnOutcome.Key? = nil, routeID: String, grantRevision: UInt64) async throws -> ThreadFollowupReceipt {
        guard validID(routeID), grantRevision > 0 else { throw ThreadTaskError.invalidTask }
        let receipt = try await followupExchange(operation, parent: parent, originalKey: originalKey,
            submission: ["route_id": routeID, "grant_revision": grantRevision])
        guard let receipt else { close(); throw ThreadTaskError.invalidReceipt }
        return receipt
    }

    /// Read-only recovery: an absent receipt never creates a queue or sends input.
    public func threadFollowupReceipt(for operation: ThreadFollowupOperation, parent: ThreadTaskOutboxOperation,
        originalKey: MobileTurnOutcome.Key? = nil) async throws -> ThreadFollowupReceipt? {
        try await followupExchange(operation, parent: parent, originalKey: originalKey, submission: nil)
    }

    private func followupExchange(_ operation: ThreadFollowupOperation, parent: ThreadTaskOutboxOperation,
        originalKey: MobileTurnOutcome.Key?, submission: [String: Any]?) async throws -> ThreadFollowupReceipt? {
        guard capabilities.contains("thread_task_followup_v1") else { throw ThreadTaskError.followupRequiresUpgrade }
        guard let grant, parent.machineID == machineID, parent.deviceID == grant.deviceID else { throw MobileError.invalidCredential }
        try operation.validate(parent: parent, originalKey: originalKey)
        do {
            var fields = submission ?? [:]
            fields["followup"] = try JSONSerialization.jsonObject(with: JSONEncoder().encode(operation))
            let reply = try await exchange(type: submission == nil ? "thread_task_followup_receipt" : "thread_task_followup", fields: fields)
            guard reply.result?.status == "thread_followup_receipt" else { throw ThreadTaskError.invalidReceipt }
            let received = reply.result?.threadFollowup
            try received?.validate(operation: operation, parent: parent, originalKey: originalKey)
            return received
        } catch { close(); throw error }
    }
}
