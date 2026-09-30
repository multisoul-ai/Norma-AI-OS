import Foundation
import SQLite3

public struct ThreadFollowupOutboxOperation: Codable, Equatable, Identifiable, Sendable {
    public var id: String { followup.operationID }
    public let machineID: String
    public let followup: ThreadFollowupOperation
    public internal(set) var receipt: ThreadFollowupReceipt?
    public internal(set) var replyMessageID: String?
    public var awaitingReceipt: Bool {
        guard let receipt else { return true }
        if !receipt.queuedMessage.isTerminal { return true }
        return receipt.queuedMessage.state == "sent" && (receipt.outcome == nil || receipt.delivery == nil)
    }
}

extension CollaborationWorkspaceStore {
    /// A follow-up owns a new source message, but never creates another dispatch or Agent.
    public func appendThreadFollowup(parentOperationID: String, messageID: String, text: String,
                                     operationID: String, clearingDraftRevision: String? = nil) throws -> ThreadFollowupOutboxOperation {
        try threadDatabase(writing: true) { db in
            let parent = try readTaskOutbox(db, operationID: parentOperationID)
            try requireTaskTarget(db, parent)
            _ = try appendMessage(db, threadID: parent.task.threadID, id: messageID, text: text, at: .now,
                                  clearingDraftRevision: clearingDraftRevision)
            let source = try prepareThreadDelivery(db, messageID: messageID)
            guard source.machineID == parent.machineID, let message = source.message else { throw ThreadDeliveryError.targetChanged }
            let payload = try ThreadFollowupOperation(operationID: operationID, parent: parent, source: message)
            try payload.validate(parent: parent)
            if let row = try threadRows(db, "SELECT id,thread_id,payload FROM thread_followup_outbox WHERE id=?", [operationID]).first {
                let existing = try decodeFollowup(db, row)
                guard existing.followup == payload, existing.machineID == parent.machineID else { throw WorkThreadError.conflict }
                return existing
            }
            let operation = ThreadFollowupOutboxOperation(machineID: parent.machineID, followup: payload)
            try writeFollowup(db, operation)
            return operation
        }
    }

    public func threadFollowup(operationID: String) throws -> ThreadFollowupOutboxOperation {
        try threadDatabase { try readFollowup($0, operationID: operationID) }
    }

    public func threadFollowups(threadID: String) throws -> [ThreadFollowupOutboxOperation] {
        try threadDatabase { db in
            _ = try requireThread(db, threadID)
            return try threadRows(db, "SELECT id,thread_id,payload FROM thread_followup_outbox WHERE thread_id=? ORDER BY rowid", [threadID])
                .map { try decodeFollowup(db, $0) }
        }
    }

    public func pendingThreadFollowups() throws -> [ThreadFollowupOutboxOperation] {
        try threadDatabase { db in
            try threadRows(db, "SELECT id,thread_id,payload FROM thread_followup_outbox ORDER BY rowid", [])
                .map { try decodeFollowup(db, $0) }.filter(\.awaitingReceipt)
        }
    }

    public func prepareThreadFollowup(operationID: String, deviceID: String) throws -> ThreadFollowupOutboxOperation {
        try threadDatabase { db in
            let operation = try readFollowup(db, operationID: operationID)
            let parent = try readTaskOutbox(db, operationID: operation.followup.parentOperationID)
            try requireTaskDevice(parent, deviceID: deviceID)
            try requireTaskTarget(db, parent)
            let source = try deliveryMessage(db, id: operation.followup.sourceMessageID)
            guard source.state == .delivered else { throw ThreadTaskError.sourceMissing }
            guard let row = try threadRows(db, "SELECT id,id,configuration FROM collaboration_workspaces WHERE id=?", [source.workspaceID]).first,
                  let frozen = try readThreadOutbox(db, messageID: source.id)?.message else { throw WorkThreadError.damagedStore }
            let workspace = try decode(CollaborationWorkspace.self, row)
            let projects = Set(workspace.projects.filter { $0.machineID == operation.machineID }.map(\.projectID))
            guard frozen.projectIDs.allSatisfy(projects.contains) else { throw ThreadDeliveryError.targetChanged }
            return operation
        }
    }

    public func applyThreadFollowupReceipt(_ receipt: ThreadFollowupReceipt) throws {
        try threadDatabase(writing: true) { db in
            var operation = try readFollowup(db, operationID: receipt.receipt.operationID)
            let parent = try readTaskOutbox(db, operationID: operation.followup.parentOperationID)
            try receipt.validate(operation: operation.followup, parent: parent, originalKey: operation.receipt?.originalKey)
            guard try deliveryMessage(db, id: operation.followup.sourceMessageID).state == .delivered else { throw ThreadTaskError.sourceMissing }
            let merged = try receipt.merging(operation.receipt)
            operation.receipt = merged
            if operation.replyMessageID == nil, let text = merged.outcome?.summary,
               !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                let reply = WorkThreadMessage(id: UUID().uuidString, workspaceID: operation.followup.collaborationWorkspaceID,
                    threadID: operation.followup.threadID, author: "agent", originDeviceID: operation.machineID,
                    text: text, createdAt: .now, state: .delivered, taskOperationID: parent.id,
                    replyToMessageID: operation.followup.sourceMessageID)
                try insertThreadRecord(db, "INSERT INTO work_messages(id,workspace_id,thread_id,payload) VALUES(?,?,?,?)",
                                       [reply.id, reply.workspaceID, reply.threadID], reply)
                operation.replyMessageID = reply.id
            }
            try writeFollowup(db, operation)
        }
    }

    func validateFollowupReply(_ db: OpaquePointer, _ message: WorkThreadMessage) throws {
        guard let source = message.replyToMessageID,
              let row = try threadRows(db, "SELECT id,thread_id,payload FROM thread_followup_outbox WHERE source_message_id=?", [source]).first else {
            throw WorkThreadError.damagedStore
        }
        let operation = try decodeFollowup(db, row)
        guard operation.replyMessageID == message.id, operation.machineID == message.originDeviceID,
              operation.followup.parentOperationID == message.taskOperationID,
              operation.followup.threadID == message.threadID, operation.followup.collaborationWorkspaceID == message.workspaceID,
              operation.receipt?.outcome?.summary == message.text, message.state == .delivered else { throw WorkThreadError.damagedStore }
    }

    private func readFollowup(_ db: OpaquePointer, operationID: String) throws -> ThreadFollowupOutboxOperation {
        guard let row = try threadRows(db, "SELECT id,thread_id,payload FROM thread_followup_outbox WHERE id=?", [operationID]).first else { throw WorkThreadError.notFound }
        return try decodeFollowup(db, row)
    }

    private func writeFollowup(_ db: OpaquePointer, _ operation: ThreadFollowupOutboxOperation) throws {
        try insertThreadRecord(db, "INSERT INTO thread_followup_outbox(id,thread_id,source_message_id,parent_operation_id,payload) VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",
            [operation.id, operation.followup.threadID, operation.followup.sourceMessageID, operation.followup.parentOperationID], operation)
    }

    private func decodeFollowup(_ db: OpaquePointer, _ row: (id: String, scope: String, data: Data)) throws -> ThreadFollowupOutboxOperation {
        let operation = try decode(ThreadFollowupOutboxOperation.self, row)
        let followup = operation.followup
        let parent = try readTaskOutbox(db, operationID: followup.parentOperationID)
        try followup.validate(parent: parent)
        guard operation.machineID == parent.machineID, followup.threadID == row.scope,
              let source = try readThreadOutbox(db, messageID: followup.sourceMessageID),
              let message = source.message, source.machineID == operation.machineID,
              followup == (try ThreadFollowupOperation(operationID: operation.id, parent: parent, source: message)),
              try !threadRows(db, "SELECT id,thread_id,payload FROM thread_followup_outbox WHERE id=? AND source_message_id=? AND parent_operation_id=?",
                              [operation.id, followup.sourceMessageID, parent.id]).isEmpty else { throw WorkThreadError.damagedStore }
        if let receipt = operation.receipt {
            guard source.receipt != nil else { throw WorkThreadError.damagedStore }
            try receipt.validate(operation: followup, parent: parent)
        }
        let text = operation.receipt?.outcome?.summary
        let hasReply = text.map { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty } ?? false
        guard hasReply == (operation.replyMessageID != nil) else { throw WorkThreadError.damagedStore }
        if let id = operation.replyMessageID {
            guard let row = try threadRows(db, "SELECT id,workspace_id,payload FROM work_messages WHERE id=?", [id]).first else { throw WorkThreadError.damagedStore }
            let reply = try decode(WorkThreadMessage.self, row)
            guard reply.author == "agent", reply.state == .delivered, reply.originDeviceID == operation.machineID,
                  reply.workspaceID == row.scope, reply.workspaceID == followup.collaborationWorkspaceID,
                  reply.threadID == followup.threadID, reply.taskOperationID == parent.id,
                  reply.replyToMessageID == followup.sourceMessageID, reply.text == text else { throw WorkThreadError.damagedStore }
        }
        return operation
    }
}

extension ThreadFollowupReceipt {
    func merging(_ previous: Self?) throws -> Self {
        guard let previous else { return self }
        guard binding == previous.binding, fingerprint == previous.fingerprint, receipt == previous.receipt,
              originalKey == previous.originalKey else { throw ThreadTaskError.invalidReceipt }
        var result = self
        if previous.queuedMessage.isTerminal {
            if !queuedMessage.isTerminal { return previous }
            guard queuedMessage == previous.queuedMessage else { throw ThreadTaskError.invalidReceipt }
        } else if previous.queuedMessage.state == "dispatching", queuedMessage.state == "queued" {
            return previous
        }
        if let known = previous.outcome {
            guard outcome == nil || outcome == known else { throw ThreadTaskError.invalidReceipt }
            result.outcome = known
        }
        if let known = previous.delivery {
            guard delivery == nil || delivery == known else { throw ThreadTaskError.invalidReceipt }
            result.delivery = known
        }
        return result
    }
}
