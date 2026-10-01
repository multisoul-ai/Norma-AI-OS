import Foundation

extension CollaborationWorkspaceStore {
    /// A second tap reuses the original decision. A definitive rejection requires an explicit retry.
    public func requestThreadAcceptance(operationID: String, retryRejected: Bool = false) throws -> ThreadTaskOutboxOperation {
        try threadDatabase(writing: true) { db in
            var operation = try readTaskOutbox(db, operationID: operationID)
            guard operation.revisionOperationID == nil else { throw WorkThreadError.conflict }
            if let current = operation.acceptance,
               !(retryRejected && current.receipt?.state == "rejected" && current.acceptance == nil) { return operation }
            try requireTaskTarget(db, operation)
            operation.acceptance = try ThreadAcceptanceState(decision: ThreadAcceptanceDecision(operation: operation))
            try writeTaskOutbox(db, operation)
            return operation
        }
    }

    public func applyThreadAcceptance(_ incoming: ThreadAcceptanceState, operationID: String) throws {
        try threadDatabase(writing: true) { db in
            var operation = try readTaskOutbox(db, operationID: operationID)
            guard let previous = operation.acceptance, previous.decision == incoming.decision else { throw MobileError.invalidResponse }
            try incoming.validate(for: operation)
            var merged = incoming
            if let receipt = previous.receipt {
                if let next = incoming.receipt {
                    guard receipt.routeID == next.routeID else { throw MobileError.invalidResponse }
                    if receipt.state != "accepted" {
                        guard next.state == "accepted" || receipt == next else { throw MobileError.invalidResponse }
                        merged.receipt = receipt
                    }
                } else { merged.receipt = receipt }
            }
            if let accepted = previous.acceptance {
                guard incoming.acceptance == nil || incoming.acceptance == accepted else { throw MobileError.invalidResponse }
                merged.acceptance = accepted
            }
            try merged.validate(for: operation)
            operation.acceptance = merged
            try writeTaskOutbox(db, operation)
        }
    }

    /// Query before submitting. A missing remote record cannot erase an already observed receipt.
    public func deliverThreadAcceptance(operationID: String, using connection: CollaborationRelayConnection) async throws {
        let operation = try threadTask(operationID: operationID)
        guard let state = operation.acceptance else { throw MobileError.invalidResponse }
        let existing = try await connection.threadTaskAcceptance(for: operation)
        if existing.receipt != nil || existing.acceptance != nil {
            try applyThreadAcceptance(existing, operationID: operationID)
            return
        }
        guard state.receipt == nil else { throw ThreadTaskError.receiptMissing }
        let snapshot = try await connection.snapshot()
        guard snapshot.grant.projects.contains(where: { $0.workspaceID == operation.task.projectID && $0.access == "control" }) else {
            throw MobileError.unauthorized
        }
        guard let project = snapshot.projects.first(where: { $0.workspaceID == operation.task.projectID }),
              project.state == "open", let route = project.routeID else { throw MobileError.unavailable }
        let current = try threadDatabase { db in
            let current = try readTaskOutbox(db, operationID: operationID)
            try requireTaskTarget(db, current)
            return current
        }
        guard current.acceptance?.decision == state.decision else { throw WorkThreadError.conflict }
        if current.acceptance?.receipt != nil || current.acceptance?.acceptance != nil { return }
        let result = try await connection.threadTaskAcceptance(for: current, routeID: route, grantRevision: snapshot.grant.revision)
        try applyThreadAcceptance(result, operationID: operationID)
    }
}
