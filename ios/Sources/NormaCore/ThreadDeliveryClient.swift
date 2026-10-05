import Foundation

extension CollaborationRelayConnection {
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
