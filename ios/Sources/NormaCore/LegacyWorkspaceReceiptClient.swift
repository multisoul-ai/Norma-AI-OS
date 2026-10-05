import Foundation

extension MobileClient {
    /// Lookup only: a missing old receipt never submits or reconstructs a legacy task.
    public func legacyThreadReceipt(_ machine: PairedMachine, message: ThreadMessageOperation) async throws -> ThreadMessageReceipt? {
        try message.validate(machineID: machine.id)
        let reply = try await legacyReceipt(machine, type: "thread_receipt", workspaceID: message.collaborationWorkspaceID,
                                            threadID: message.threadID, operationID: message.operationID)
        guard reply.result?.status == "thread_receipt" else { throw MobileError.invalidResponse }
        if let receipt = reply.result?.threadReceipt { try receipt.validate(message: message, machineID: machine.id) }
        return reply.result?.threadReceipt
    }
    public func legacyTaskReceipt(_ machine: PairedMachine, task: ThreadTaskOperation) async throws -> ThreadTaskReceipt? {
        try task.validate(machineID: machine.id)
        guard let device = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        let reply = try await legacyReceipt(machine, type: "thread_task_receipt", workspaceID: task.collaborationWorkspaceID,
                                            threadID: task.threadID, operationID: task.operationID)
        guard reply.result?.status == "thread_task_receipt" else { throw MobileError.invalidResponse }
        if let receipt = reply.result?.threadTask { try receipt.validate(task: task, machineID: machine.id, deviceID: device.deviceID) }
        return reply.result?.threadTask
    }
    private func legacyReceipt(_ machine: PairedMachine, type: String, workspaceID: String, threadID: String, operationID: String) async throws -> WireReply {
        guard let device = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        return try await exchange(machine, action: ["type": type, "device_id": device.deviceID, "token": device.token,
            "collaboration_workspace_id": workspaceID, "thread_id": threadID, "operation_id": operationID])
    }
}
