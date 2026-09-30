import Foundation

extension RealtimeBindingClient {
    public func deliveryPage(scope: ProjectReviewRequest, before: UInt64?) async throws -> TaskDeliveryPage {
        try scope.validate()
        guard before.map({ $0 > 0 && $0 <= Int64.max }) ?? true else { throw MobileError.invalidResponse }
        var action: [String: Any] = ["type":"task_deliveries",
            "request":try JSONSerialization.jsonObject(with: JSONEncoder().encode(scope))]
        if let before { action["before"] = before }
        let data = try await recoveryRead(action)
        struct Envelope: Decodable { let result: Result; struct Result: Decodable { let status: String; let deliveries: TaskDeliveryPage } }
        let value = try JSONDecoder().decode(Envelope.self, from: data).result
        guard value.status == "task_deliveries" else { throw MobileError.invalidResponse }
        try value.deliveries.validate(for: scope, nodeID: nil, before: before)
        return value.deliveries
    }
    public func operations(workspaceID: String, after: UInt64 = 0) async throws -> [RealtimeOperationObservation] {
        guard validID(workspaceID), after <= Int64.max else { throw MobileError.invalidResponse }
        let data = try await recoveryRead(["type":"realtime_operations", "workspace_id":workspaceID, "after":after])
        struct Envelope: Decodable {
            let result: Page
            struct Page: Decodable { let status: String; let grant: DeviceGrant; let operations: [RealtimeOperation] }
        }
        let page = try JSONDecoder().decode(Envelope.self, from: data).result
        try page.grant.validate(deviceID: credential.deviceID)
        guard page.status == "realtime_operations", page.operations.count <= 20,
              page.grant.projects.contains(where: { $0.id == workspaceID }) else { throw MobileError.invalidResponse }
        var sequence = after
        return try page.operations.map { operation in
            try operation.validate()
            guard operation.deviceID == credential.deviceID, operation.workspaceID == workspaceID,
                  operation.sequence > sequence else { throw MobileError.invalidResponse }
            sequence = operation.sequence
            return try RealtimeOperationObservation(machineID: machine.id, operation: operation)
        }
    }
    public func receipt(for record: RealtimeOperationObservation) async throws -> RealtimeOperationObservation {
        try record.validate()
        guard record.machineID == machine.id, record.operation.deviceID == credential.deviceID else { throw MobileError.invalidResponse }
        let data = try await recoveryRead(["type":"receipt", "workspace_id":record.operation.workspaceID,
                                          "operation_id":record.operation.operationID])
        guard let value = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let result = value["result"] as? [String: Any], result["status"] as? String == "receipt",
              let receipt = result["receipt"] as? [String: Any] else { throw MobileError.invalidResponse }
        return try record.applying(receipt: JSONSerialization.data(withJSONObject: receipt))
    }
    private func recoveryRead(_ action: [String: Any]) async throws -> Data {
        try machine.validate()
        let id = UUID().uuidString
        var action = action
        action["device_id"] = credential.deviceID; action["token"] = credential.token
        let body = try JSONSerialization.data(withJSONObject: ["version":1,"request_id":id,
            "machine_id":machine.id,"action":action])
        return try await Self.readWithRecovery {
            let connection = try await PinnedConnection.open(machine)
            defer { connection.close() }
            let data = try await connection.exchange(body)
            _ = try WireReply.decode(data, expectedID: id)
            return data
        }
    }
    // Shared only by the three read endpoints above; never wrap a task submission in this retry.
    static func readWithRecovery(_ read: @Sendable () async throws -> Data) async throws -> Data {
        var attempts = 0
        while true {
            try Task.checkCancellation()
            do { return try await read() }
            catch {
                try Task.checkCancellation()
                let transient: [MobileError] = [.connectionFailed, .timedOut, .unavailable]
                guard attempts < 3, (error as? MobileError).map(transient.contains) ?? (error is URLError) else { throw error }
                // Reuse the feeds' 1+2+4s recovery ceiling; every attempt verifies the same pinned Mac.
                try await Task.sleep(for: .seconds(1 << attempts)); attempts += 1
            }
        }
    }
}
