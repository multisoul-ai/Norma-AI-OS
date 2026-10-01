import Foundation

extension CollaborationWorkspaceStore {
    /// One task or follow-up per foreground pass, sharing the existing relay budget and rotation.
    public func deliverPendingThreadTasks(using connection: CollaborationRelayConnection) async throws -> (operationID: String, error: String?)? {
        let tasks = try pendingThreadTasks(includingExecution: true).filter { $0.machineID == connection.machineID }
        let followups = try pendingThreadFollowups().filter { $0.machineID == connection.machineID }
        var queue = tasks.map { (id: $0.id, followup: false) } + followups.map { (id: $0.id, followup: true) }
        if let previous = lastTaskAttempt[connection.machineID], let index = queue.firstIndex(where: { $0.id == previous }) {
            queue = Array(queue.dropFirst(index + 1)) + Array(queue.prefix(index + 1))
        }
        guard let operation = queue.first else { return nil }
        try Task.checkCancellation()
        lastTaskAttempt[connection.machineID] = operation.id
        do {
            if operation.followup {
                try await deliverThreadFollowup(operationID: operation.id, using: connection)
            } else if tasks.first(where: { $0.id == operation.id })?.acceptance?.awaitingReceipt == true {
                try await deliverThreadAcceptance(operationID: operation.id, using: connection)
            } else {
                try await deliverThreadTask(operationID: operation.id, using: connection)
            }
            return (operation.id, nil)
        } catch is CancellationError { throw CancellationError() }
        catch { return (operation.id, error.localizedDescription) }
    }

    /// Recover the same queue first. A missing known receipt is never permission to resend input.
    @discardableResult public func deliverThreadFollowup(operationID: String, using connection: CollaborationRelayConnection) async throws -> ThreadFollowupReceipt {
        let operation = try threadFollowup(operationID: operationID)
        guard operation.machineID == connection.machineID else { throw ThreadDeliveryError.targetChanged }
        let parent = try threadTask(operationID: operation.followup.parentOperationID)
        guard let grant = await connection.grant else { throw MobileError.unauthorized }
        try requireTaskDevice(parent, deviceID: grant.deviceID)
        if !operation.awaitingReceipt, let receipt = operation.receipt { return receipt }
        if let existing = try await connection.threadFollowupReceipt(for: operation.followup, parent: parent,
                                                                     originalKey: operation.receipt?.originalKey) {
            try applyThreadFollowupReceipt(existing)
            return try threadFollowup(operationID: operationID).receipt!
        }
        guard try threadFollowup(operationID: operationID).receipt == nil else { throw ThreadTaskError.receiptMissing }
        _ = try prepareThreadFollowup(operationID: operationID, deviceID: grant.deviceID)
        let snapshot = try await connection.snapshot()
        guard snapshot.grant.projects.contains(where: { $0.workspaceID == parent.task.projectID && $0.access == "control" }) else {
            throw MobileError.unauthorized
        }
        guard let project = snapshot.projects.first(where: { $0.workspaceID == parent.task.projectID }),
              project.state == "open", let route = project.routeID else { throw MobileError.unavailable }
        guard route == parent.receipt?.receipt.routeID else { throw MobileError.sessionChanged }
        let current = try prepareThreadFollowup(operationID: operationID, deviceID: grant.deviceID)
        if let receipt = current.receipt { return receipt }
        let receipt = try await connection.submitThreadFollowup(current.followup, parent: parent,
            routeID: route, grantRevision: snapshot.grant.revision)
        try applyThreadFollowupReceipt(receipt)
        return try threadFollowup(operationID: operationID).receipt!
    }

    /// Resolve the original operation before any new dispatch, including after app restart.
    @discardableResult public func deliverThreadTask(operationID: String, using connection: CollaborationRelayConnection) async throws -> ThreadTaskReceipt {
        let operation = try threadTask(operationID: operationID)
        guard operation.machineID == connection.machineID else { throw ThreadDeliveryError.targetChanged }
        guard let grant = await connection.grant else { throw MobileError.unauthorized }
        try requireTaskDevice(operation, deviceID: grant.deviceID)
        if !operation.awaitsDispatchReceipt && !operation.awaitsExecutionReceipt, let receipt = operation.receipt { return receipt }
        if operation.awaitsExecutionReceipt, !(await connection.capabilities).contains("thread_task_results_v1") {
            throw ThreadTaskError.resultsRequireUpgrade
        }
        if let existing = try await connection.threadTaskReceipt(for: operation.task) {
            try applyThreadTaskReceipt(existing, deviceID: grant.deviceID)
            return try threadTask(operationID: operationID).receipt!
        }
        // Losing the Mac's command ledger must never turn known reception into permission to resend.
        guard try threadTask(operationID: operationID).receipt == nil else { throw ThreadTaskError.receiptMissing }
        _ = try prepareThreadTask(operationID: operationID, deviceID: grant.deviceID)
        let snapshot = try await connection.snapshot()
        guard snapshot.grant.projects.contains(where: { $0.workspaceID == operation.task.projectID && $0.access == "control" }) else {
            throw MobileError.unauthorized
        }
        guard let project = snapshot.projects.first(where: { $0.workspaceID == operation.task.projectID }),
              project.state == "open", let route = project.routeID else { throw MobileError.unavailable }
        // Membership may change during either network wait; the frozen task never follows a new target.
        let current = try prepareThreadTask(operationID: operationID, deviceID: grant.deviceID)
        if let receipt = current.receipt { return receipt }
        let receipt = try await connection.submitThreadTask(operation.task, routeID: route, grantRevision: snapshot.grant.revision)
        try applyThreadTaskReceipt(receipt, deviceID: grant.deviceID)
        return try threadTask(operationID: operationID).receipt!
    }
}
