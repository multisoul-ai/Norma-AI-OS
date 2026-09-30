import Foundation
import SQLite3

extension CollaborationWorkspaceStore {
    public func pendingThreadDeliveries() throws -> [ThreadOutboxOperation] {
        try threadDatabase(writing: true) { db in
            // Upgrade only messages without an outbox; never replace an existing operation ID.
            let legacy = try threadRows(db, "SELECT m.id,m.workspace_id,m.payload FROM work_messages m LEFT JOIN thread_outbox o ON o.message_id=m.id WHERE o.id IS NULL", [])
            for row in legacy {
                let message = try decode(WorkThreadMessage.self, row)
                try validateMessageAuthor(db, message)
                if message.author == "user" { _ = try ensureThreadOutbox(db, message: message) }
            }
            return try threadRows(db, "SELECT id,message_id,payload FROM thread_outbox ORDER BY rowid", [])
                .map { try decodeOutbox(db, row: $0) }.filter { $0.receipt == nil }
        }
    }

    public func prepareThreadDelivery(messageID: String) throws -> ThreadOutboxOperation {
        try threadDatabase(writing: true) { try prepareThreadDelivery($0, messageID: messageID) }
    }

    func prepareThreadDelivery(_ db: OpaquePointer, messageID: String) throws -> ThreadOutboxOperation {
        let original = try deliveryMessage(db, id: messageID)
        var operation = try ensureThreadOutbox(db, message: original)
        if operation.receipt != nil { return operation }
        let thread = try requireThread(db, original.threadID)
        try requireWorkspace(db, thread.workspaceID, writing: true)
        guard let row = try threadRows(db, "SELECT id,id,configuration FROM collaboration_workspaces WHERE id=?", [thread.workspaceID]).first else { throw WorkThreadError.notFound }
        let workspace = try decode(CollaborationWorkspace.self, row)
        try validate(workspace)
        if let frozen = operation.message, let mac = operation.machineID {
            let current = Set(workspace.projects.filter { $0.machineID == mac }.map(\.projectID))
            guard frozen.projectIDs.allSatisfy(current.contains) else { throw ThreadDeliveryError.targetChanged }
            return operation
        }
        let machines = Set(workspace.projects.map(\.machineID))
        guard !machines.isEmpty else { throw ThreadDeliveryError.unconfigured }
        guard machines.count == 1, let mac = machines.first else { throw ThreadDeliveryError.multipleMachines }
        let payload = ThreadMessageOperation(operationID: operation.id, collaborationWorkspaceID: thread.workspaceID,
            threadID: thread.id, messageID: original.id, originDeviceID: original.originDeviceID,
            projectIDs: workspace.projects.map(\.projectID).sorted(), text: original.text)
        try payload.validate(machineID: mac)
        operation.machineID = mac; operation.message = payload
        try writeOutbox(db, operation)
        return operation
    }

    public func applyThreadReceipt(_ receipt: ThreadMessageReceipt) throws {
        try threadDatabase(writing: true) { db in
            guard let row = try threadRows(db, "SELECT id,message_id,payload FROM thread_outbox WHERE id=?", [receipt.operationID]).first else { throw ThreadDeliveryError.invalidReceipt }
            var operation = try decodeOutbox(db, row: row)
            guard let message = operation.message, let mac = operation.machineID else { throw ThreadDeliveryError.invalidReceipt }
            try receipt.validate(message: message, machineID: mac)
            if let previous = operation.receipt {
                guard previous == receipt else { throw ThreadDeliveryError.invalidReceipt }
                return
            }
            let original = try deliveryMessage(db, id: operation.messageID)
            let delivered = WorkThreadMessage(id: original.id, workspaceID: original.workspaceID,
                threadID: original.threadID, author: original.author, originDeviceID: original.originDeviceID,
                text: original.text, createdAt: original.createdAt, state: .delivered)
            operation.receipt = receipt
            try writeOutbox(db, operation)
            try insertThreadRecord(db, "UPDATE work_messages SET payload=?2 WHERE id=?1", [original.id], delivered)
            guard sqlite3_changes(db) == 1 else { throw WorkThreadError.damagedStore }
        }
    }

    func ensureThreadOutbox(_ db: OpaquePointer, message: WorkThreadMessage) throws -> ThreadOutboxOperation {
        guard message.author == "user", message.taskOperationID == nil else { throw WorkThreadError.invalidContent }
        if let existing = try readThreadOutbox(db, messageID: message.id) { return existing }
        guard message.state == .pending else { throw WorkThreadError.damagedStore }
        let operation = ThreadOutboxOperation(id: UUID().uuidString, messageID: message.id)
        try writeOutbox(db, operation)
        return operation
    }

    func readThreadOutbox(_ db: OpaquePointer, messageID: String) throws -> ThreadOutboxOperation? {
        guard let row = try threadRows(db, "SELECT id,message_id,payload FROM thread_outbox WHERE message_id=?", [messageID]).first else { return nil }
        return try decodeOutbox(db, row: row)
    }

    private func writeOutbox(_ db: OpaquePointer, _ operation: ThreadOutboxOperation) throws {
        try insertThreadRecord(db, "INSERT INTO thread_outbox(id,message_id,payload) VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", [operation.id, operation.messageID], operation)
    }

    private func decodeOutbox(_ db: OpaquePointer, row: (id: String, scope: String, data: Data)) throws -> ThreadOutboxOperation {
        let operation = try decode(ThreadOutboxOperation.self, row)
        let original = try deliveryMessage(db, id: operation.messageID)
        guard operation.messageID == row.scope, (operation.machineID == nil) == (operation.message == nil) else { throw WorkThreadError.damagedStore }
        if let message = operation.message, let mac = operation.machineID {
            try message.validate(machineID: mac)
            guard message.operationID == operation.id, message.messageID == original.id,
                  message.collaborationWorkspaceID == original.workspaceID, message.threadID == original.threadID,
                  message.originDeviceID == original.originDeviceID, message.text == original.text else { throw WorkThreadError.damagedStore }
            if let receipt = operation.receipt { try receipt.validate(message: message, machineID: mac) }
        } else if operation.receipt != nil { throw WorkThreadError.damagedStore }
        guard (operation.receipt != nil) == (original.state == .delivered) else { throw WorkThreadError.damagedStore }
        return operation
    }

    func deliveryMessage(_ db: OpaquePointer, id: String) throws -> WorkThreadMessage {
        guard let row = try threadRows(db, "SELECT id,workspace_id,payload FROM work_messages WHERE id=?", [id]).first else { throw WorkThreadError.notFound }
        let message = try decode(WorkThreadMessage.self, row)
        let thread = try requireThread(db, message.threadID)
        guard message.workspaceID == row.scope, message.workspaceID == thread.workspaceID,
              message.author == "user", message.taskOperationID == nil, UUID(uuidString: message.originDeviceID) != nil,
              !message.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { throw WorkThreadError.damagedStore }
        return message
    }
}
