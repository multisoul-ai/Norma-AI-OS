import Foundation
import NormaCore

@MainActor enum TodoCheck {
    /// 数据：真实忙碌Agent、三项固定计划；无预写决定，原生成之外只允许两个Todo回合。
    /// 过程：保存/入队 → 原生撤回 → 读可重排 → 停止原生成 → 显式重排/拒绝/队列 → 准确历史与终态。
    /// 预期：取消项起初未送达；原入队回执不变且无turn；重排只执行一次，旧撤回不影响新回合。
    static func run(client: MobileClient, machine: PairedMachine) async throws {
        let catalog = try await client.snapshot(machine)
        guard let project = catalog.projects.first, let route = project.routeID else { throw MobileError.invalidResponse }
        let observation = Observation()
        let reader = Task {
            do {
                for try await update in client.planUpdates(machine, workspaceID: project.id, planID: "native-plan") {
                    switch update {
                    case .current(let progress): observation.latest = progress
                    case .empty: throw MobileError.invalidResponse
                    case .legacy: throw MobileError.invalidResponse
                    case .recovering: observation.latest = nil
                    }
                }
            } catch { if !Task.isCancelled { observation.failure = error } }
        }
        defer { reader.cancel() }
        let progress = try await observation.wait { _ in true }
        guard progress.decisions.isEmpty else {
            print("TODO_EXPECTS_NO_SEEDED_DECISION")
            throw MobileError.invalidResponse
        }
        guard progress.plan.draft.todos.count == 3 else { throw MobileError.invalidResponse }
        print("TODO_STAGE plan-read")
        let plan = progress.plan
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("todos.json")
        let store = PendingCommandStore(url: url)
        let before = try await client.snapshot(machine, workspaceID: project.id)
        guard let busy = before.projects.first?.cards?.first, busy.isBusy, busy.turnID != nil else { throw MobileError.invalidResponse }
        let original = try await client.prepareTodo(machine, routeID: route, revision: before.grant.revision,
            plan: plan, todoIndex: 0, choice: .accept(target: busy, queue: true))
        try await store.save(original)
        _ = try await client.submit(original, to: machine)
        let initial = try await MessageCheck.receipt(client, machine, original)
        guard initial.state == "succeeded", initial.result?.status == "queued", initial.result?.turnID == nil else { throw MobileError.invalidResponse }
        let waiting = try await client.projectPlan(machine, workspaceID: project.id, planID: plan.id)
        guard waiting.decisions.count == 1, waiting.decisions[0].deliveryState == "queued",
              !waiting.decisions[0].canRetry, waiting.decisions[0].needsFollowUp,
              waiting.decisions[0].queuedMessage?.result == nil else { throw MobileError.invalidResponse }
        let cancelled = try await client.cancelTodoQueue(original, on: machine, revision: before.grant.revision)
        guard cancelled.state == "cancelled", cancelled.result == nil, cancelled.text == plan.draft.todos[0].instruction else { throw MobileError.invalidResponse }
        let cancelledProgress = try await observation.wait { $0.decisions.first?.deliveryState == "cancelled" }
        guard let previous = cancelledProgress.decisions.first, previous.todoIndex == 0,
              previous.reference.operationID == original.request.operationID, previous.reference.deviceID == original.deviceID,
              previous.deliveryState == "cancelled", previous.canRetry, previous.needsFollowUp, previous.receipt.state == "succeeded",
              previous.receipt.result?.status == "queued", previous.receipt.result?.turnID == nil,
              previous.queuedMessage?.result == nil, previous.outcome == nil, previous.retryOf == nil else { throw MobileError.invalidResponse }
        try await store.remove(id: original.id) // Same reconciliation as the UI: confirmed cancellation, server record remains.
        print("TODO_STAGE actual-queue-cancelled")
        let stop = try await client.prepareInterrupt(machine, workspaceID: project.id, routeID: route, revision: before.grant.revision, card: busy)
        try await store.save(stop)
        _ = try await client.submit(stop, to: machine)
        let stopped = try await MessageCheck.receipt(client, machine, stop)
        print("TODO_STOP state=\(stopped.state) status=\(stopped.result?.status ?? "none") turn_matches=\(stopped.result?.turnID == busy.turnID)")
        guard stopped.state == "succeeded", stopped.result?.status == "interrupted", stopped.result?.turnID == busy.turnID else { throw MobileError.invalidResponse }
        try await store.remove(id: stop.id)
        let afterStop = try await client.snapshot(machine, workspaceID: project.id)
        print("TODO_AFTER_STOP state=\(afterStop.projects.first?.cards?.first?.state ?? "none") run_matches=\(afterStop.projects.first?.cards?.first?.runID == busy.runID) session_matches=\(afterStop.projects.first?.cards?.first?.sessionID == busy.sessionID)")
        guard let idle = afterStop.projects.first?.cards?.first, !idle.isBusy,
              let runBeforeRetry = idle.runID, let sessionBeforeRetry = idle.sessionID,
              runBeforeRetry == busy.runID, sessionBeforeRetry == busy.sessionID else { throw MobileError.invalidResponse }
        let baseline = try await client.conversation(machine, request: ConversationRequest(workspaceID: project.id, routeID: route,
            nodeID: idle.id, runID: runBeforeRetry, sessionID: sessionBeforeRetry, cursor: nil)).page.items.filter { $0.role == "user" }.map(\.text)
        print("TODO_BASELINE count=\(baseline.count) priming_matches=\(baseline.first?.hasPrefix("Without tools, begin your reply") == true) unexpected_delivery=\(baseline.contains(plan.draft.todos[0].instruction))")
        // One priming task; Claude additionally records its native interruption as one system-generated user item.
        guard baseline.count == (idle.profile == "Claude" ? 2 : 1),
              baseline.first?.hasPrefix("Without tools, begin your reply") == true,
              idle.profile != "Claude" || baseline.last == "[Request interrupted by user]",
              !baseline.contains(plan.draft.todos[0].instruction) else { throw MobileError.invalidResponse }
        print("TODO_STAGE no-delivery-before-retry")
        let first = try await client.prepareTodo(machine, routeID: route, revision: afterStop.grant.revision, plan: plan, todoIndex: 0, choice: .accept(target: idle, queue: false), retryOf: previous.reference)
        try await store.save(first)
        _ = try await client.submit(first, to: machine)
        let restored = try await PendingCommandStore(url: url).load()
        guard restored == [first] else { throw MobileError.invalidResponse }
        let firstReceipt = try await MessageCheck.receipt(client, machine, restored[0])
        guard firstReceipt.state == "succeeded", let run = firstReceipt.result?.runID, let session = firstReceipt.result?.sessionID,
              run == runBeforeRetry, session == sessionBeforeRetry, firstReceipt.result?.turnID != busy.turnID else { throw MobileError.invalidResponse }
        print("TODO_STAGE first-sent")
        let reject = try await client.prepareTodo(machine, routeID: route, revision: catalog.grant.revision, plan: plan, todoIndex: 1, choice: .reject)
        try await store.save(reject)
        let rejection = try await client.submit(reject, to: machine)
        guard rejection.result?.status == "declined", rejection.result?.nodeID == nil else { throw MobileError.invalidResponse }
        print("TODO_STAGE declined")
        let snapshot = try await client.snapshot(machine, workspaceID: project.id)
        guard let active = snapshot.projects.first?.cards?.first, active.runID == run, active.sessionID == session else { throw MobileError.invalidResponse }
        // The second decision explicitly binds the latest state of the same running session.
        let queued = try await client.prepareTodo(machine, routeID: route, revision: snapshot.grant.revision, plan: plan, todoIndex: 2, choice: .accept(target: active, queue: true))
        try await store.save(queued)
        let ack = try await client.submit(queued, to: machine)
        guard ack.result?.status == "queued" else { throw MobileError.invalidResponse }
        print("TODO_STAGE queue-accepted")
        let sentProgress = try await observation.wait { $0.decisions.first(where: { $0.id == 2 })?.deliveryState == "sent" }
        let delivery = sentProgress.decisions.first { $0.id == 2 }
        guard delivery?.deliveryState == "sent", delivery?.deliveryResult?.runID == run,
              delivery?.deliveryResult?.sessionID == session, delivery?.deliveryResult?.turnID != firstReceipt.result?.turnID else { throw MobileError.invalidResponse }
        print("TODO_STAGE queue-sent")
        let done = try await MessageCheck.completed(client, machine, project.id, route, active.id, "NORMA_TODO_SECOND")
        print("TODO_STAGE second-finished")
        let history = try await client.conversation(machine, request: ConversationRequest(workspaceID: project.id, routeID: route, nodeID: done.id, runID: run, sessionID: session, cursor: nil)).page
        guard history.items.filter({ $0.role == "user" }).map(\.text) == baseline + [plan.draft.todos[0].instruction, plan.draft.todos[2].instruction],
              history.items.contains(where: { $0.role == "assistant" && $0.text.trimmingCharacters(in: .whitespacesAndNewlines) == "NORMA_TODO_FIRST" }) else { throw MobileError.invalidResponse }
        print("TODO_STAGE history-verified")
        let duplicate = try await client.submit(first, to: machine)
        guard duplicate.result?.turnID == firstReceipt.result?.turnID else { throw MobileError.invalidResponse }
        let conflicting = try await client.prepareTodo(machine, routeID: route, revision: catalog.grant.revision, plan: plan, todoIndex: 0, choice: .reject)
        do { _ = try await client.submit(conflicting, to: machine); throw MobileError.invalidResponse }
        catch MobileError.todoDecided { }
        print("TODO_STAGE decisions-verified")
        let final = try await observation.wait { $0.decisions.first(where: { $0.id == 2 })?.outcome != nil }
        print("NATIVE_PLAN_STREAM_OUTCOMES_OK")
        print("TODO_STAGE outcomes-read")
        print("TODO_OUTCOMES \(final.decisions.map { $0.outcome?.state.rawValue ?? "none" })")
        guard final.decisions.count == 3, final.decisions[1].deliveryState == "declined", final.decisions[1].outcome == nil,
              final.decisions[0].outcome?.state == .completed, final.decisions[2].outcome?.state == .completed,
              final.decisions[0].outcome?.key.turnID == firstReceipt.result?.turnID,
              final.decisions[2].outcome?.key.turnID == delivery?.deliveryResult?.turnID else { throw MobileError.invalidResponse }
        guard !final.decisions[0].needsFollowUp, !final.decisions[1].needsFollowUp, !final.decisions[2].needsFollowUp else { throw MobileError.invalidResponse }
        print("NATIVE_TODO_PENDING_LIST_OK")
        guard !final.decisions[0].canRetry, !final.decisions[1].canRetry, !final.decisions[2].canRetry,
              final.decisions[0].retryOf == previous.reference,
              final.decisions[0].reference != previous.reference,
              final.decisions[1].retryOf == nil, final.decisions[2].retryOf == nil else { throw MobileError.invalidResponse }
        let retrySent = try await client.prepareTodo(machine, routeID: route, revision: catalog.grant.revision,
            plan: plan, todoIndex: 0, choice: .accept(target: done, queue: false), retryOf: final.decisions[0].reference)
        do { _ = try await client.submit(retrySent, to: machine); throw MobileError.invalidResponse }
        catch MobileError.todoDecided { }
        let originalReceipt = try await client.submit(original, to: machine)
        guard originalReceipt.state == initial.state, originalReceipt.result?.status == "queued",
              originalReceipt.result?.queueID == initial.result?.queueID, originalReceipt.result?.turnID == nil else { throw MobileError.invalidResponse }
        let oldCancel = try await client.cancelTodoQueue(original, on: machine, revision: catalog.grant.revision)
        guard oldCancel.state == "cancelled", oldCancel.result == nil, oldCancel.text == cancelled.text else { throw MobileError.invalidResponse }
        let afterReplay = try await client.projectPlan(machine, workspaceID: project.id, planID: plan.id)
        guard afterReplay.decisions[0].reference == final.decisions[0].reference,
              afterReplay.decisions[0].outcome?.key.turnID == firstReceipt.result?.turnID,
              afterReplay.decisions[0].outcome?.state == .completed else { throw MobileError.invalidResponse }
        print("NATIVE_REAL_TODO_CANCEL_RETRY \(active.profile)")
        print("NATIVE_REAL_TODO_RETRY_REJECTED \(active.profile)")
        print("NATIVE_REAL_TODO_RETRY_SENT_ONCE \(active.profile)")
        print("NATIVE_REAL_TODOS_OK \(active.profile)")
        print("NATIVE_REAL_TODO_OUTCOMES_OK \(active.profile)")
        reader.cancel(); await reader.value
        print("NATIVE_PLAN_STREAM_CANCEL_OK")
    }
}

private extension TodoCheck {
    @MainActor final class Observation {
        var latest: ProjectPlanProgress?
        var failure: (any Error)?
        func wait(_ condition: (ProjectPlanProgress) -> Bool) async throws -> ProjectPlanProgress {
            //45s covers the existing two short real Provider replies and queue dispatch; no new model tasks.
            let deadline = ContinuousClock.now.advanced(by: .seconds(45))
            while true {
                if let failure { throw failure }
                if let latest, condition(latest) { return latest }
                guard ContinuousClock.now < deadline else { throw MobileError.timedOut }
                try await Task.sleep(for: .milliseconds(50))
            }
        }
    }
}
