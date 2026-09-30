import Foundation
import NormaCore

enum MessageCheck {
    @MainActor static func run(client: MobileClient, machine: PairedMachine, measuring: Bool = false, nativeCard: Bool = false) async throws {
        let catalog = try await client.snapshot(machine)
        guard catalog.projects.count == 1 else { throw MobileError.invalidResponse }
        let workspace = catalog.projects[0].id
        let first = try await client.snapshot(machine, workspaceID: workspace)
        guard let project = first.projects.first, let route = project.routeID, let cards = project.cards, cards.count == 1,
              let empty = cards.first, nativeCard ? (empty.state == "ready" && empty.runID != nil) : (empty.state == "idle" && empty.runID == nil) else { throw MobileError.invalidResponse }
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = PendingCommandStore(url: directory.appendingPathComponent("messages.json"))
        let text = " \n\tDo not use any tools, read or modify files.\nReply exactly:\t\tNORMA_MOBILE_FIRST\t \n"
        let draftStore = ChatDraftStore(url: directory.appendingPathComponent("drafts.json"))
        let book = ChatDraftBook(store: draftStore)
        let emptyScope = ChatDraft.Scope(machineID: machine.id, workspaceID: workspace, routeID: route, nodeID: empty.id, runID: empty.runID, sessionID: empty.sessionID)
        await book.load(); book.update(text, for: emptyScope); try await book.flush()
        let restoredBook = ChatDraftBook(store: draftStore); await restoredBook.load()
        guard let draft = restoredBook.draft(for: emptyScope), draft.text == text else { throw MobileError.invalidResponse }
        let pending = try await client.prepareMessage(machine, workspaceID: workspace, routeID: route, revision: first.grant.revision, card: empty, text: draft.text, draftID: draft.id)
        try await store.save(pending)
        _ = try await client.submit(pending, to: machine) // Simulate losing this first response.
        let restored = try await store.load()
        guard restored.count == 1 else { throw MobileError.invalidResponse }
        let received = try await receipt(client, machine, restored[0])
        print("MESSAGE_FIRST_RECEIPT \(received.state) \(received.error ?? "none")")
        guard received.state == "succeeded", let initial = received.result else { throw MobileError.invalidResponse }
        try await restoredBook.confirm(restored[0], receipt: received)
        guard restoredBook.draft(for: emptyScope) == nil else { throw MobileError.invalidResponse }
        let one = try await completed(client, machine, workspace, route, empty.id, "NORMA_MOBILE_FIRST")
        let deliveryScope = ProjectReviewRequest(workspaceID: workspace, routeID: route)
        let firstDeliveries = try await client.taskDeliveries(machine, scope: deliveryScope, nodeID: empty.id)
        guard firstDeliveries.items.count == 1, let firstDelivery = firstDeliveries.items.first,
              firstDelivery.outcome.key.turnID == initial.turnID, firstDelivery.outcome.state == .completed else { throw MobileError.invalidResponse }
        print("MESSAGE_FIRST_COMPLETE")
        let watch = ConversationWatchCheck(client: client, machine: machine, request: ConversationRequest(workspaceID: workspace, routeID: route, nodeID: one.id, runID: one.runID!, sessionID: one.sessionID!, cursor: nil))
        defer { watch.cancel() }
        try await watch.require("NORMA_MOBILE_FIRST")
        if measuring {
            try await NativeTiming.idleChat(provider: one.profile)
            var samples: [Duration] = []
            let scope = ConversationRequest(workspaceID: workspace, routeID: route, nodeID: one.id, runID: one.runID!, sessionID: one.sessionID!, cursor: nil)
            for _ in 0..<20 {
                let started = ContinuousClock.now
                let page = try await client.conversation(machine, request: scope).page
                let duration = ContinuousClock.now - started
                guard page.items.contains(where: { $0.role == "assistant" && $0.text.trimmingCharacters(in: .whitespacesAndNewlines) == "NORMA_MOBILE_FIRST" }),
                      !page.items.contains(where: { $0.role == "assistant" && $0.text == "NORMA_MOBILE_SECOND" }) else { throw MobileError.invalidResponse }
                samples.append(duration)
            }
            try NativeTiming.report("chat_page_read", provider: one.profile, samples: samples,
                scope: "native Keychain + new TLS/WSS + existing real Provider history + complete page decode; not first token or first fragment")
        }
        guard one.runID == initial.runID, one.sessionID == initial.sessionID else { throw MobileError.invalidResponse }
        let again = try await client.submit(pending, to: machine)
        guard again.result?.turnID == initial.turnID else { throw MobileError.invalidResponse }
        let nextText = "Do not use any tools, read or modify files.\nReply exactly:\tNORMA_MOBILE_SECOND"
        let activeScope = ChatDraft.Scope(machineID: machine.id, workspaceID: workspace, routeID: route, nodeID: one.id, runID: one.runID, sessionID: one.sessionID)
        restoredBook.update(nextText, for: activeScope); try await restoredBook.flush()
        guard let nextDraft = restoredBook.draft(for: activeScope) else { throw MobileError.invalidResponse }
        let next = try await client.prepareMessage(machine, workspaceID: workspace, routeID: route, revision: first.grant.revision, card: one, text: nextDraft.text, draftID: nextDraft.id)
        try await store.save(next)
        let replyStarted = ContinuousClock.now
        _ = try await client.submit(next, to: machine)
        let second = try await receipt(client, machine, next)
        print("MESSAGE_SECOND_RECEIPT \(second.state) \(second.error ?? "none")")
        guard second.state == "succeeded", second.result?.sessionID == initial.sessionID,
              second.result?.runID == initial.runID, second.result?.turnID != initial.turnID else { throw MobileError.invalidResponse }
        try await restoredBook.confirm(next, receipt: second)
        let remainingDrafts = try await draftStore.load()
        guard remainingDrafts.isEmpty else { throw MobileError.invalidResponse }
        print("NATIVE_REAL_DRAFTS_OK \(empty.profile)")
        let done = try await completed(client, machine, workspace, route, empty.id, "NORMA_MOBILE_SECOND")
        let secondDeliveries = try await client.taskDeliveries(machine, scope: deliveryScope, nodeID: empty.id)
        guard secondDeliveries.items.count == 2, secondDeliveries.items[0].outcome.key.turnID == second.result?.turnID,
              secondDeliveries.items[1].fingerprint == firstDelivery.fingerprint else { throw MobileError.invalidResponse }
        print("NATIVE_REAL_TASK_DELIVERIES_OK \(empty.profile)")
        print("MESSAGE_SECOND_COMPLETE")
        let history = try await client.conversation(machine, request: ConversationRequest(workspaceID: workspace, routeID: route, nodeID: done.id, runID: done.runID!, sessionID: done.sessionID!, cursor: nil)).page
        let messages = history.items.filter { $0.role == "user" }
        // Match native editors only: Claude expands Tab; Codex trims outer whitespace on submission.
        let expected = [text, nextText].map { empty.profile == "Claude" ? $0.replacingOccurrences(of: "\t", with: "    ") : nativeCard ? $0.trimmingCharacters(in: .whitespacesAndNewlines) : $0 }
        guard messages.map(\.text) == expected else { throw MobileError.invalidResponse }
        try await watch.require("NORMA_MOBILE_SECOND")
        if measuring, let observed = watch.assistantArrivals["NORMA_MOBILE_SECOND"] {
            try NativeTiming.report("live_assistant_reply", provider: one.profile, samples: [observed - replyStarted],
                scope: "1 real turn, native submit to first observed complete assistant message; includes Provider/model generation and stream processing, not pure network latency")
        }
        guard watch.timeline.items.filter({ $0.role == "user" }).map(\.text) == expected else { throw MobileError.invalidResponse }
        await watch.finish()
        print("NATIVE_REAL_CHAT_EVENTS_OK \(empty.profile)")
        let stale = try await client.prepareMessage(machine, workspaceID: workspace, routeID: route, revision: first.grant.revision, card: empty, text: "Must never be sent")
        _ = try await client.submit(stale, to: machine)
        let rejected = try await receipt(client, machine, stale)
        guard rejected.state == "rejected", rejected.error == "card_changed" else { throw MobileError.invalidResponse }
        print("NATIVE_REAL_MESSAGES_OK \(empty.profile)")
    }
    static func receipt(_ client: MobileClient, _ machine: PairedMachine, _ pending: PendingMobileCommand) async throws -> MobileCommandReceipt {
        // 45s = server send budget40s (including baseline) + receipt/network allowance5s; query only.
        let deadline = ContinuousClock.now.advanced(by: .seconds(45))
        while ContinuousClock.now < deadline {
            let result = try await client.receipt(for: pending, on: machine)
            if result.state != "accepted" { return result }
            try await Task.sleep(for: .milliseconds(100))
        }
        throw MobileError.timedOut
    }
    static func completed(_ client: MobileClient, _ machine: PairedMachine, _ workspace: String, _ route: String, _ node: UInt64, _ expected: String) async throws -> CardSnapshot {
        //30s actual reply +10s artifact capture +5s scheduling =45s; sending is still a separate receipt.
        let deadline = ContinuousClock.now.advanced(by: .seconds(45))
        var lastState: String?
        while ContinuousClock.now < deadline {
            let snapshot = try await client.snapshot(machine, workspaceID: workspace)
            let current = snapshot.projects.first?.cards?.first(where: { $0.id == node })
            if current?.state != lastState { lastState = current?.state; print("MESSAGE_CARD_STATE \(lastState ?? "missing")") }
            if let card = current, card.state == "done", let run = card.runID, let session = card.sessionID {
                let page = try await client.conversation(machine, request: ConversationRequest(workspaceID: workspace, routeID: route, nodeID: node, runID: run, sessionID: session, cursor: nil)).page
                if page.items.contains(where: { $0.role == "assistant" && $0.text.trimmingCharacters(in: .whitespacesAndNewlines) == expected }) { return card }
            }
            try await Task.sleep(for: .milliseconds(100))
        }
        throw MobileError.timedOut
    }
}
