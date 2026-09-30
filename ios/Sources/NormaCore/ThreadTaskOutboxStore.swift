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

extension CollaborationWorkspaceStore {
    /// Save a direct call and its frozen task in the same transaction, clearing only its own draft.
    public func appendTaskMessage(threadID: String, messageID: String, text: String, operationID: String,
                                  target: WorkspaceAgentReference, task: String, contextVersion: UInt64,
                                  context: String, clearingDraftRevision: String? = nil) throws -> ThreadTaskOutboxOperation {
        try threadDatabase(writing: true) { db in
            _ = try appendMessage(db, threadID: threadID, id: messageID, text: text, at: .now,
                                  clearingDraftRevision: clearingDraftRevision)
            return try enqueueThreadTask(db, sourceMessageID: messageID, operationID: operationID, target: target,
                                         task: task, contextVersion: contextVersion, context: context)
        }
    }

    public func enqueueThreadTask(sourceMessageID: String, operationID: String, target: WorkspaceAgentReference,
                                  task: String, contextVersion: UInt64, context: String, revisionOf: ThreadTaskRevision? = nil) throws -> ThreadTaskOutboxOperation {
        try threadDatabase(writing: true) {
            try enqueueThreadTask($0, sourceMessageID: sourceMessageID, operationID: operationID, target: target,
                                  task: task, contextVersion: contextVersion, context: context, revisionOf: revisionOf)
        }
    }

    public func threadTasks(threadID: String) throws -> [ThreadTaskOutboxOperation] {
        try threadDatabase { db in
            _ = try requireThread(db, threadID)
            return try threadRows(db, "SELECT id,thread_id,payload FROM thread_task_outbox WHERE thread_id=? ORDER BY rowid", [threadID])
                .map { try decodeTaskOutbox(db, row: $0) }
        }
    }

    public func pendingThreadTasks(includingExecution: Bool = false) throws -> [ThreadTaskOutboxOperation] {
        try threadDatabase { db in
            try threadRows(db, "SELECT id,thread_id,payload FROM thread_task_outbox ORDER BY rowid", [])
                .map { try decodeTaskOutbox(db, row: $0) }
                .filter { $0.awaitsDispatchReceipt || (includingExecution && ($0.awaitsExecutionReceipt || $0.acceptance?.awaitingReceipt == true)) }
        }
    }

    public func threadTask(operationID: String) throws -> ThreadTaskOutboxOperation {
        try threadDatabase { try readTaskOutbox($0, operationID: operationID) }
    }

    /// Before a new send, recheck membership and pin the paired device identity durably.
    public func prepareThreadTask(operationID: String, deviceID: String) throws -> ThreadTaskOutboxOperation {
        try threadDatabase(writing: true) { db in
            var operation = try readTaskOutbox(db, operationID: operationID)
            try requireTaskDevice(operation, deviceID: deviceID)
            try requireTaskTarget(db, operation)
            let source = try deliveryMessage(db, id: operation.task.sourceMessageID)
            guard source.state == .delivered else { throw ThreadTaskError.sourceMissing }
            if operation.deviceID == nil {
                operation.deviceID = deviceID
                try writeTaskOutbox(db, operation)
            }
            return operation
        }
    }

    public func applyThreadTaskReceipt(_ receipt: ThreadTaskReceipt, deviceID: String, at date: Date = .now) throws {
        try threadDatabase(writing: true) { db in
            var operation = try readTaskOutbox(db, operationID: receipt.receipt.operationID)
            try requireTaskDevice(operation, deviceID: deviceID)
            try receipt.validate(task: operation.task, machineID: operation.machineID, deviceID: deviceID)
            var merged = receipt
            if let previous = operation.receipt {
                guard previous.receipt.routeID == receipt.receipt.routeID else { throw ThreadTaskError.invalidReceipt }
                // A late accepted reply cannot roll a terminal dispatch receipt back into the send queue.
                if previous.receipt.state != "accepted" {
                    if receipt.receipt.state == "accepted" { return }
                    guard previous.receipt == receipt.receipt else { throw ThreadTaskError.invalidReceipt }
                    merged = previous
                }
                merged.execution = try receipt.execution?.merging(previous.execution) ?? previous.execution
            }
            operation.deviceID = deviceID; operation.receipt = merged
            let progress = ThreadTaskProgress(merged, at: date)
            try progress.validate(through: merged)
            if operation.progress?.last?.sameState(as: progress) != true {
                operation.progress = (operation.progress ?? []) + [progress]
            }
            if operation.replyMessageID == nil, let text = merged.execution?.execution.result?.text,
               !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                let reply = WorkThreadMessage(id: UUID().uuidString, workspaceID: operation.task.collaborationWorkspaceID,
                    threadID: operation.task.threadID, author: "agent", originDeviceID: operation.machineID,
                    text: text, createdAt: .now, state: .delivered, taskOperationID: operation.id)
                try insertThreadRecord(db, "INSERT INTO work_messages(id,workspace_id,thread_id,payload) VALUES(?,?,?,?)",
                                       [reply.id, reply.workspaceID, reply.threadID], reply)
                operation.replyMessageID = reply.id
            }
            try writeTaskOutbox(db, operation)
        }
    }

    func requireTaskDevice(_ operation: ThreadTaskOutboxOperation, deviceID: String) throws {
        guard validID(deviceID), operation.deviceID == nil || operation.deviceID == deviceID else { throw MobileError.invalidCredential }
    }

    func readTaskOutbox(_ db: OpaquePointer, operationID: String) throws -> ThreadTaskOutboxOperation {
        guard let row = try threadRows(db, "SELECT id,thread_id,payload FROM thread_task_outbox WHERE id=?", [operationID]).first else { throw WorkThreadError.notFound }
        return try decodeTaskOutbox(db, row: row)
    }

    func enqueueThreadTask(_ db: OpaquePointer, sourceMessageID: String, operationID: String,
                                   target: WorkspaceAgentReference, task: String, contextVersion: UInt64,
                                   context: String, revisionOf: ThreadTaskRevision? = nil) throws -> ThreadTaskOutboxOperation {
        let source = try deliveryMessage(db, id: sourceMessageID)
        let sourceOutbox = try ensureThreadOutbox(db, message: source)
        var payload = ThreadTaskOperation(operationID: operationID, collaborationWorkspaceID: source.workspaceID,
            threadID: source.threadID, sourceOperationID: sourceOutbox.id, sourceMessageID: source.id,
            projectID: target.projectID, agentDefinitionID: target.definitionID, task: task,
            contextVersion: contextVersion, context: context)
        payload.revisionOf = revisionOf
        try payload.validate(machineID: target.machineID)
        if let row = try threadRows(db, "SELECT id,thread_id,payload FROM thread_task_outbox WHERE id=?", [operationID]).first {
            let original = try decodeTaskOutbox(db, row: row)
            guard original.machineID == target.machineID, original.task == payload else { throw WorkThreadError.conflict }
            return original
        }
        _ = try prepareThreadDelivery(db, messageID: source.id)
        var operation = ThreadTaskOutboxOperation(machineID: target.machineID, task: payload)
        operation.target = target
        if let row = try threadRows(db, "SELECT id,id,configuration FROM collaboration_workspaces WHERE id=?", [source.workspaceID]).first {
            operation.projectName = try decode(CollaborationWorkspace.self, row).projects.first { $0.id == target.projectReferenceID }?.name
        }
        try requireTaskTarget(db, operation)
        try writeTaskOutbox(db, operation)
        return operation
    }

    func requireTaskTarget(_ db: OpaquePointer, _ operation: ThreadTaskOutboxOperation) throws {
        let task = operation.task
        try requireWorkspace(db, task.collaborationWorkspaceID, writing: true)
        guard let row = try threadRows(db, "SELECT id,id,configuration FROM collaboration_workspaces WHERE id=?", [task.collaborationWorkspaceID]).first else { throw WorkThreadError.notFound }
        let workspace = try decode(CollaborationWorkspace.self, row)
        try validate(workspace)
        guard let outbox = try readThreadOutbox(db, messageID: task.sourceMessageID) else { throw WorkThreadError.damagedStore }
        guard outbox.machineID == operation.machineID, let message = outbox.message,
              message.projectIDs.contains(task.projectID) else { throw ThreadTaskError.projectOutsideThread }
        let current = Set(workspace.projects.filter { $0.machineID == operation.machineID }.map(\.projectID))
        guard message.projectIDs.allSatisfy(current.contains),
              workspace.agents.contains(where: { $0.machineID == operation.machineID && $0.projectID == task.projectID && $0.definitionID == task.agentDefinitionID }) else {
            throw ThreadDeliveryError.targetChanged
        }
    }

    func writeTaskOutbox(_ db: OpaquePointer, _ operation: ThreadTaskOutboxOperation) throws {
        try insertThreadRecord(db, "INSERT INTO thread_task_outbox(id,thread_id,payload) VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",
                               [operation.id, operation.task.threadID], operation)
    }

    private func decodeTaskOutbox(_ db: OpaquePointer, row: (id: String, scope: String, data: Data)) throws -> ThreadTaskOutboxOperation {
        let operation = try decode(ThreadTaskOutboxOperation.self, row)
        let task = operation.task
        try task.validate(machineID: operation.machineID)
        let source = try deliveryMessage(db, id: task.sourceMessageID)
        guard let outbox = try readThreadOutbox(db, messageID: source.id) else { throw WorkThreadError.damagedStore }
        guard task.threadID == row.scope, task.threadID == source.threadID, task.collaborationWorkspaceID == source.workspaceID,
              task.sourceOperationID == outbox.id, outbox.machineID == operation.machineID,
              outbox.message?.projectIDs.contains(task.projectID) == true,
              operation.deviceID.map(validID) ?? true else { throw WorkThreadError.damagedStore }
        if let target = operation.target {
            guard target.machineID == operation.machineID, target.projectID == task.projectID, target.definitionID == task.agentDefinitionID,
                  !target.name.isEmpty, target.name.count <= 256, target.profile.count <= 256 else { throw WorkThreadError.damagedStore }
        }
        guard operation.projectName.map({ !$0.isEmpty && $0.count <= 256 }) ?? true else { throw WorkThreadError.damagedStore }
        if let receipt = operation.receipt {
            guard let device = operation.deviceID else { throw WorkThreadError.damagedStore }
            try receipt.validate(task: task, machineID: operation.machineID, deviceID: device)
        }
        if let progress = operation.progress {
            guard let receipt = operation.receipt, let last = progress.last,
                  Set(progress.map(\.id)).count == progress.count,
                  last.sameState(as: ThreadTaskProgress(receipt, at: last.observedAt)) else { throw WorkThreadError.damagedStore }
            var previous: ThreadTaskProgress?
            for item in progress {
                try item.validate(through: receipt)
                guard previous?.sameState(as: item) != true,
                      previous?.executionSequence == nil || (item.executionSequence ?? 0) > previous!.executionSequence! else {
                    throw WorkThreadError.damagedStore
                }
                previous = item
            }
        }
        let text = operation.receipt?.execution?.execution.result?.text
        let hasReply = text.map { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty } ?? false
        guard hasReply == (operation.replyMessageID != nil) else { throw WorkThreadError.damagedStore }
        if let id = operation.replyMessageID {
            guard let row = try threadRows(db, "SELECT id,workspace_id,payload FROM work_messages WHERE id=?", [id]).first else { throw WorkThreadError.damagedStore }
            let reply = try decode(WorkThreadMessage.self, row)
            guard reply.taskOperationID == operation.id, reply.author == "agent", reply.state == .delivered,
                  reply.originDeviceID == operation.machineID, reply.workspaceID == row.scope,
                  reply.workspaceID == task.collaborationWorkspaceID, reply.threadID == task.threadID,
                  reply.text == text else { throw WorkThreadError.damagedStore }
        }
        try operation.terminalOperation?.validate(for: operation)
        guard operation.terminalDraft.map({ $0.valid && $0.mentions == nil && $0.followupTaskID == nil }) ?? true else { throw WorkThreadError.damagedStore }
        try operation.acceptance?.validate(for: operation)
        guard operation.reviewDraft.map({ $0.valid && $0.mentions == nil && $0.followupTaskID == nil }) ?? true,
              operation.revisionOperationID.map({ UUID(uuidString: $0) != nil && $0 != operation.id }) ?? true else { throw WorkThreadError.damagedStore }
        return operation
    }
}
