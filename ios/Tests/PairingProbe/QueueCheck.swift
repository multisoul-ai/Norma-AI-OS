import Foundation
import NormaCore

enum QueueCheck {
    /// Real Provider fixture: queue three during generation, cancel the second, then stop the
    /// first task. Only the Mac dispatcher sends the remaining two, in order, with no phone replay.
    @MainActor static func run(client: MobileClient, machine: PairedMachine) async throws {
        let catalog = try await client.snapshot(machine)
        guard catalog.projects.count == 1 else { throw MobileError.invalidResponse }
        let workspace = catalog.projects[0].id
        let snapshot = try await client.snapshot(machine, workspaceID: workspace)
        guard let project = snapshot.projects.first, let route = project.routeID,
              let card = project.cards?.first, card.isBusy else { throw MobileError.invalidResponse }
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("queue.json")
        let store = PendingCommandStore(url: url)
        let book = ChatDraftBook(store: ChatDraftStore(url: directory.appendingPathComponent("drafts.json")))
        await book.load()
        let scope = ChatDraft.Scope(machineID: machine.id, workspaceID: workspace, routeID: route, nodeID: card.id, runID: card.runID, sessionID: card.sessionID)
        var queued: [PendingMobileCommand] = []
        for marker in ["NORMA_QUEUE_FIRST", "NORMA_QUEUE_CANCELLED", "NORMA_QUEUE_LAST"] {
            let text = "Do not use any tools, read or modify files.\nReply exactly:\t" + marker
            book.update(text, for: scope); try await book.flush()
            guard let draft = book.draft(for: scope) else { throw MobileError.invalidResponse }
            let message = try await client.prepareQueuedMessage(machine, workspaceID: workspace, routeID: route, revision: snapshot.grant.revision, card: card, text: draft.text, draftID: draft.id)
            try await store.save(message)
            _ = try await client.submit(message, to: machine) // Lose initial acknowledgement.
            let acknowledgement = try await client.receipt(for: message, on: machine)
            guard acknowledgement.state == "succeeded", acknowledgement.result?.status == "queued",
                  acknowledgement.result?.turnID == nil else { throw MobileError.invalidResponse }
            try await book.confirm(message, receipt: acknowledgement)
            guard book.draft(for: scope) == nil else { throw MobileError.invalidResponse }
            let waiting = try await client.queuedMessage(message, on: machine)
            guard waiting.state == "queued", waiting.result == nil else { throw MobileError.invalidResponse }
            queued.append(message)
        }
        let cancelled = try await client.cancelQueuedMessage(queued[1], on: machine, revision: snapshot.grant.revision)
        guard cancelled.state == "cancelled", cancelled.text.contains("NORMA_QUEUE_CANCELLED"), cancelled.result == nil else { throw MobileError.invalidResponse }
        print("QUEUE_WAIT_CANCEL_OK \(card.profile)")
        let stop = try await client.prepareInterrupt(machine, workspaceID: workspace, routeID: route, revision: snapshot.grant.revision, card: card)
        try await store.save(stop)
        _ = try await client.submit(stop, to: machine)
        let stopped = try await MessageCheck.receipt(client, machine, stop)
        guard stopped.state == "succeeded", stopped.result?.turnID == card.turnID else { throw MobileError.invalidResponse }
        // Fresh store/client recovery performs only reads. 60s = two short model replies at 30s each.
        let restored = try await PendingCommandStore(url: url).load().filter { $0.request.command.isQueuedMessage }
        guard restored == queued else { throw MobileError.invalidResponse }
        let deadline = ContinuousClock.now.advanced(by: .seconds(60))
        var firstTurn: String?
        var lastTurn: String?
        while ContinuousClock.now < deadline {
            let first = try await client.queuedMessage(restored[0], on: machine)
            let last = try await client.queuedMessage(restored[2], on: machine)
            print("QUEUE_STATES \(card.profile) \(first.state) \(last.state)")
            if last.state == "sent" {
                guard first.state == "sent" else { throw MobileError.invalidResponse }
                firstTurn = first.result?.turnID; lastTurn = last.result?.turnID; break
            }
            guard !["cancelled", "outcome_unknown"].contains(first.state), !last.isTerminal else { throw MobileError.invalidResponse }
            try await Task.sleep(for: .milliseconds(500))
        }
        guard let firstTurn, let lastTurn, firstTurn != lastTurn, firstTurn != card.turnID else { throw MobileError.timedOut }
        print("QUEUE_DELIVERIES_CONFIRMED \(card.profile)")
        let done = try await MessageCheck.completed(client, machine, workspace, route, card.id, "NORMA_QUEUE_LAST")
        print("QUEUE_LAST_COMPLETE \(card.profile)")
        for message in queued {
            let repeated = try await client.submit(message, to: machine)
            guard repeated.result?.status == "queued", repeated.result?.turnID == nil else { throw MobileError.invalidResponse }
        }
        let remainsCancelled = try await client.queuedMessage(queued[1], on: machine)
        guard remainsCancelled.state == "cancelled" else { throw MobileError.invalidResponse }
        do {
            _ = try await client.cancelQueuedMessage(queued[0], on: machine, revision: snapshot.grant.revision)
            throw MobileError.invalidResponse
        } catch MobileError.queueNotCancellable {}
        print("QUEUE_REPLAY_AND_LATE_CANCEL_OK \(card.profile)")
        let page = try await client.conversation(machine, request: ConversationRequest(workspaceID: workspace, routeID: route, nodeID: card.id, runID: done.runID!, sessionID: done.sessionID!, cursor: nil)).page
        let messages = page.items.filter { $0.role == "user" }.map(\.text)
        let expected = [queued[0], queued[2]].compactMap { $0.request.command.messageContent?.text }.map { card.profile == "Claude" ? $0.replacingOccurrences(of: "\t", with: "    ") : $0 }
        // Claude records the native interruption as a synthetic user item, in addition to the
        // original task and two queue messages: 1 + 1 + 2 = 4 (Codex: 1 + 0 + 2 = 3).
        guard messages.count == (card.profile == "Claude" ? 4 : 3),
              card.profile != "Claude" || messages[1] == "[Request interrupted by user]", Array(messages.suffix(2)) == expected,
              !messages.contains(where: { $0.contains("NORMA_QUEUE_CANCELLED") }),
              page.items.contains(where: { $0.role == "assistant" && $0.text.trimmingCharacters(in: .whitespacesAndNewlines) == "NORMA_QUEUE_FIRST" }) else { throw MobileError.invalidResponse }
        print("NATIVE_REAL_QUEUES_OK \(card.profile)")
    }
}
