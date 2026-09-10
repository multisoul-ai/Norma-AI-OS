import Foundation
import NormaCore

enum FeedbackCheck {
    @MainActor static func run(client: MobileClient, machine: PairedMachine, directory: URL) async throws {
        let catalog = try await client.snapshot(machine)
        guard let workspace = catalog.projects.first?.id else { throw MobileError.invalidResponse }
        let snapshot = try await client.snapshot(machine, workspaceID: workspace)
        guard let project = snapshot.projects.first, let route = project.routeID, let card = project.cards?.first else { throw MobileError.invalidResponse }
        let initial = try await client.prepareMessage(machine, workspaceID: workspace, routeID: route, revision: snapshot.grant.revision,
                                                     card: card, text: "Do not use tools or read or modify files. Reply exactly: FEEDBACK_BASE_READY")
        _ = try await client.submit(initial, to: machine)
        guard try await MessageCheck.receipt(client, machine, initial).state == "succeeded" else { throw MobileError.invalidResponse }
        let ready = try await MessageCheck.completed(client, machine, workspace, route, card.id, "FEEDBACK_BASE_READY")
        let scope = ProjectReviewRequest(workspaceID: workspace, routeID: route)
        let page = try await client.taskDeliveries(machine, scope: scope, nodeID: card.id)
        print("FEEDBACK_BASE_DELIVERIES \(page.items.count) \(page.items.first?.status ?? "missing")")
        guard let first = page.items.first, first.available else { throw MobileError.invalidResponse }
        let frozen = try await client.projectReview(machine, request: ProjectReviewRequest(workspaceID: workspace, routeID: route, deliveryKey: first.outcome.key))
        guard let delivery = frozen.delivery else { throw MobileError.invalidResponse }
        let text = "  Do not use tools or read or modify files.\nReply exactly: FEEDBACK_APPLIED  "
        let draftStore = ChatDraftStore(url: directory.appendingPathComponent("feedback-drafts.json"))
        let book = ChatDraftBook(store: draftStore); await book.load()
        let ordinaryScope = ChatDraft.Scope(machineID: machine.id, workspaceID: workspace, routeID: route, nodeID: card.id, runID: ready.runID, sessionID: ready.sessionID)
        let feedbackScope = ChatDraft.Scope(machineID: machine.id, workspaceID: workspace, routeID: route, nodeID: card.id,
                                           runID: ready.runID, sessionID: ready.sessionID, feedbackFingerprint: delivery.fingerprint)
        book.update("Keep ordinary draft", for: ordinaryScope); book.update(text, for: feedbackScope); try await book.flush()
        let restored = ChatDraftBook(store: draftStore); await restored.load()
        guard let draft = restored.draft(for: feedbackScope), draft.text == text else { throw MobileError.invalidResponse }
        let pending = try await client.prepareDeliveryFeedback(machine, workspaceID: workspace, routeID: route, revision: snapshot.grant.revision,
                                                              delivery: delivery, card: ready, text: draft.text, draftID: draft.id)
        let store = PendingCommandStore(url: directory.appendingPathComponent("feedback-commands.json"))
        try await store.save(pending)
        _ = try await client.submit(pending, to: machine)
        guard let saved = try await store.load().first else { throw MobileError.invalidResponse }
        let receipt = try await MessageCheck.receipt(client, machine, saved)
        print("FEEDBACK_RECEIPT \(receipt.state) \(receipt.error ?? "none")")
        guard receipt.state == "succeeded", receipt.result?.feedback?.key == delivery.outcome.key,
              receipt.result?.feedback?.fingerprint == delivery.fingerprint,
              receipt.result?.turnID != delivery.outcome.key.turnID, receipt.result?.sessionID == ready.sessionID else { throw MobileError.invalidResponse }
        try await restored.confirm(saved, receipt: receipt)
        guard restored.draft(for: feedbackScope) == nil, restored.draft(for: ordinaryScope)?.text == "Keep ordinary draft" else { throw MobileError.invalidResponse }
        let done = try await MessageCheck.completed(client, machine, workspace, route, card.id, "FEEDBACK_APPLIED")
        let repeatReceipt = try await client.submit(saved, to: machine)
        guard repeatReceipt.result?.turnID == receipt.result?.turnID else { throw MobileError.invalidResponse }
        let history = try await client.conversation(machine, request: ConversationRequest(workspaceID: workspace, routeID: route, nodeID: card.id,
                                                                                          runID: done.runID!, sessionID: done.sessionID!, cursor: nil)).page
        let messages = history.items.filter { $0.role == "user" }
        guard messages.count == 2, let last = messages.last, last.text.hasSuffix(text), last.text.contains(delivery.fingerprint),
              last.text.contains(delivery.outcome.key.turnID) else { throw MobileError.invalidResponse }
        let old = try await client.projectReview(machine, request: frozen.request)
        guard old.delivery?.fingerprint == delivery.fingerprint, old.delivery?.acceptance == nil,
              frozen.review.files.isEmpty, old.review.files.isEmpty else { throw MobileError.invalidResponse }
        let stale = try await client.prepareDeliveryFeedback(machine, workspaceID: workspace, routeID: route, revision: snapshot.grant.revision,
                                                            delivery: delivery, card: done, text: "Must never reach a new session")
        try Data().write(to: directory.appendingPathComponent("feedback-close"))
        //5s = normal shutdown/foreground drain budget; the test host closes only this isolated Provider.
        let deadline = ContinuousClock.now.advanced(by: .seconds(5))
        while !FileManager.default.fileExists(atPath: directory.appendingPathComponent("feedback-closed").path) {
            guard ContinuousClock.now < deadline else { throw MobileError.timedOut }
            try await Task.sleep(for: .milliseconds(50))
        }
        _ = try await client.submit(stale, to: machine)
        let rejected = try await MessageCheck.receipt(client, machine, stale)
        guard rejected.state == "rejected", ["card_changed", "session_changed"].contains(rejected.error ?? "") else { throw MobileError.invalidResponse }
        print("NATIVE_REAL_DELIVERY_FEEDBACK_OK \(card.profile)")
    }
}
