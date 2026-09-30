import Foundation

extension CollaborationRelayConnection {
    public func receiveThreadMessage(_ message: ThreadMessageOperation) async throws -> ThreadMessageReceipt {
        try requireThreadDelivery()
        try message.validate(machineID: machineID)
        let value = try JSONSerialization.jsonObject(with: JSONEncoder().encode(message))
        let reply = try await exchange(type: "thread_message", fields: ["message": value])
        do {
            guard reply.result?.status == "thread_receipt", let receipt = reply.result?.threadReceipt else { throw ThreadDeliveryError.invalidReceipt }
            try receipt.validate(message: message, machineID: machineID)
            return receipt
        } catch { close(); throw error }
    }

    public func threadReceipt(for message: ThreadMessageOperation) async throws -> ThreadMessageReceipt? {
        try requireThreadDelivery()
        try message.validate(machineID: machineID)
        let reply = try await exchange(type: "thread_receipt", fields: ["collaboration_workspace_id": message.collaborationWorkspaceID,
            "thread_id": message.threadID, "operation_id": message.operationID])
        do {
            guard reply.result?.status == "thread_receipt" else { throw ThreadDeliveryError.invalidReceipt }
            if let receipt = reply.result?.threadReceipt { try receipt.validate(message: message, machineID: machineID) }
            return reply.result?.threadReceipt
        } catch { close(); throw error }
    }

    public func threadEvents(workspaceID: String, threadID: String, after: UInt64) async throws -> [ThreadMessageReceipt] {
        try requireThreadDelivery()
        guard UUID(uuidString: workspaceID) != nil, UUID(uuidString: threadID) != nil, after <= Int64.max else { throw ThreadDeliveryError.invalidMessage }
        let reply = try await exchange(type: "thread_events", fields: ["collaboration_workspace_id": workspaceID, "thread_id": threadID, "after": after])
        do {
            guard reply.result?.status == "thread_events", let events = reply.result?.threadEvents, events.count <= 32 else { throw ThreadDeliveryError.invalidReceipt }
            var cursor = after
            for event in events {
                guard event.sourceMacID == machineID, event.collaborationWorkspaceID == workspaceID, event.threadID == threadID,
                      event.cursor > cursor, event.cursor <= Int64.max, event.eventID == machineID + ":" + String(event.cursor),
                      event.state == "received", event.receivedAtUnix > 0, validHex(event.fingerprint),
                      [event.operationID, event.messageID, event.originDeviceID].allSatisfy({ UUID(uuidString: $0) != nil }) else { throw ThreadDeliveryError.invalidReceipt }
                cursor = event.cursor
            }
            return events
        } catch { close(); throw error }
    }

    private func requireThreadDelivery() throws {
        guard capabilities.contains("thread_delivery_v1") else { throw ThreadDeliveryError.requiresUpgrade }
    }
}

extension CollaborationWorkspaceStore {
    /// Resume a bounded batch, keeping errors attached to their own message.
    public func deliverPendingThreadMessages(using connection: CollaborationRelayConnection, limit: Int = 2) async throws -> [String: String] {
        var failures: [String: String] = [:]
        var attempted = 0
        var queue = try pendingThreadDeliveries()
        if let previous = lastDeliveryAttempt[connection.machineID],
           let index = queue.firstIndex(where: { $0.messageID == previous }) {
            queue = Array(queue.dropFirst(index + 1)) + Array(queue.prefix(index + 1))
        }
        for pending in queue {
            try Task.checkCancellation()
            do {
                // Do not freeze or submit an already-frozen operation to a different Mac.
                if let target = pending.machineID, target != connection.machineID { continue }
                let operation = try prepareThreadDelivery(messageID: pending.messageID)
                guard operation.machineID == connection.machineID else { continue }
                // Two query/send pairs + open/handshake/auth/close <= 8 frames.
                // App waits 5s between batches: at most 12 * 8 = 96 < 120 frames/min per Mac.
                guard attempted < min(2, max(0, limit)) else { continue }
                attempted += 1
                lastDeliveryAttempt[connection.machineID] = pending.messageID
                try await deliverThreadMessage(messageID: pending.messageID, using: connection)
            } catch is CancellationError { throw CancellationError() }
            catch { failures[pending.messageID] = error.localizedDescription }
        }
        return failures
    }

    /// Query the stable operation first. An offline/failed exchange leaves the durable outbox intact.
    @discardableResult public func deliverThreadMessage(messageID: String, using connection: CollaborationRelayConnection) async throws -> ThreadMessageReceipt {
        let operation = try prepareThreadDelivery(messageID: messageID)
        guard operation.machineID == connection.machineID, let message = operation.message else { throw ThreadDeliveryError.targetChanged }
        if let receipt = operation.receipt { return receipt }
        let receipt: ThreadMessageReceipt
        if let existing = try await connection.threadReceipt(for: message) {
            receipt = existing
        } else {
            // Workspace edits while the network was suspended cannot retarget the pending operation.
            _ = try prepareThreadDelivery(messageID: messageID)
            receipt = try await connection.receiveThreadMessage(message)
        }
        try applyThreadReceipt(receipt)
        return receipt
    }
}
