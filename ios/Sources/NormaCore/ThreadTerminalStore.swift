import Foundation

extension CollaborationWorkspaceStore {
    /// Explicit human reconciliation unlocks a future action without inventing a successful receipt.
    public func acknowledgeThreadTerminalOutcome(operationID: String, controlOperationID: String) throws {
        try threadDatabase(writing: true) { db in
            var operation = try readTaskOutbox(db, operationID: operationID)
            guard var state = operation.terminalOperation, state.action.operationID == controlOperationID,
                  state.receipt == nil || state.receipt?.state == "outcome_unknown" else { throw WorkThreadError.conflict }
            state.outcomeAcknowledged = true
            operation.terminalOperation = state
            try writeTaskOutbox(db, operation)
        }
    }

    public func saveThreadTerminalDraft(_ draft: WorkThreadDraft?, operationID: String) throws {
        try threadDatabase(writing: true) { db in
            guard draft.map({ $0.valid && $0.mentions == nil && $0.followupTaskID == nil }) ?? true else { throw WorkThreadError.invalidContent }
            var operation = try readTaskOutbox(db, operationID: operationID)
            operation.terminalDraft = draft
            try writeTaskOutbox(db, operation)
        }
    }

    public func prepareThreadTerminalAction(operationID: String, action: ThreadTerminalAction,
        terminal: ThreadTaskTerminal, clearingDraftRevision: String? = nil) throws -> ThreadTaskOutboxOperation {
        try threadDatabase(writing: true) { db in
            var operation = try readTaskOutbox(db, operationID: operationID)
            guard let receipt = operation.receipt else { throw MobileError.terminalUnavailable }
            try terminal.validate(for: ThreadTerminalRequest(task: operation.task, receipt: receipt),
                                  task: operation.task, receipt: receipt, controlled: action.type != .claim)
            let state = ThreadTerminalState(action: action, key: terminal.key, draftRevision: clearingDraftRevision)
            try state.validate(for: operation)
            if let previous = operation.terminalOperation {
                if previous.action == action, previous.key == state.key, previous.draftRevision == clearingDraftRevision { return operation }
                guard !previous.unresolved, previous.action.operationID != action.operationID else { throw WorkThreadError.conflict }
            }
            try requireTaskTarget(db, operation)
            if action.type == .input {
                guard terminal.page.inputReady, action.expected == terminal.page.card,
                      action.screenSignature == terminal.page.screenSignature, action.inputSequence == terminal.page.inputSequence else {
                    throw MobileError.terminalChanged
                }
            }
            if let clearingDraftRevision {
                guard let draft = operation.terminalDraft, draft.revision == clearingDraftRevision,
                      action.input == .text(draft.text) else { throw WorkThreadError.conflict }
            }
            operation.terminalOperation = state
            try writeTaskOutbox(db, operation)
            return operation
        }
    }

    public func applyThreadTerminalReceipt(_ receipt: MobileCommandReceipt, operationID: String) throws {
        try threadDatabase(writing: true) { db in
            var operation = try readTaskOutbox(db, operationID: operationID)
            guard var state = operation.terminalOperation else { throw MobileError.invalidResponse }
            let previous = state.receipt
            state.receipt = receipt
            try state.validate(for: operation)
            if let previous, previous.state != "accepted" {
                guard receipt.state == "accepted" || receipt == previous else { throw MobileError.invalidResponse }
                return
            }
            operation.terminalOperation = state
            if receipt.state == "succeeded", let revision = state.draftRevision,
               operation.terminalDraft?.revision == revision { operation.terminalDraft = nil }
            try writeTaskOutbox(db, operation)
        }
    }
}
