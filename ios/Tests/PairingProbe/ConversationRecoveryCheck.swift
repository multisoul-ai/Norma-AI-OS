import Foundation
import NormaCore

@MainActor enum ConversationRecoveryCheck {
    private final class Observation {
        var timeline = ConversationTimeline()
        var recovered = false
        var failure: (any Error)?
    }

    /// 真实原生WSS断流后丢弃旧页、重建完整基线，撤销授权后停止订阅。
    ///
    /// 数据构造：Rust只读fixture初始正文“中”，一次unavailable后变为“恢复后”。
    /// 执行过程：订阅 → 文件标记让测试owner断流 → recovering → 新正文 → owner撤销。
    /// 预期结果：恢复事件清空旧数据，新基线不含“中”，终态unauthorized且无写操作。
    static func run(client: MobileClient, machine: PairedMachine, directory: URL) async throws {
        let catalog = try await client.snapshot(machine)
        guard let workspace = catalog.projects.first?.id else { throw MobileError.invalidResponse }
        let snapshot = try await client.snapshot(machine, workspaceID: workspace)
        guard let project = snapshot.projects.first, let route = project.routeID, let card = project.cards?.first,
              let run = card.runID, let session = card.sessionID else { throw MobileError.invalidResponse }
        let scope = ConversationRequest(workspaceID: workspace, routeID: route, nodeID: card.id, runID: run, sessionID: session, cursor: nil)
        let state = Observation()
        let reader = Task {
            do {
                for try await update in client.conversationUpdates(machine, request: scope) {
                    switch update {
                    case .current(let page): state.timeline.merge(page, continuing: false)
                    case .heartbeat: break
                    case .recovering:
                        state.timeline = ConversationTimeline(); state.recovered = true
                    }
                }
            } catch { state.timeline = ConversationTimeline(); state.failure = error }
        }
        defer { reader.cancel() }
        try await wait { state.failure != nil || state.timeline.items.first?.text == "中" }
        if let failure = state.failure { throw failure }
        try Data().write(to: directory.appendingPathComponent("chat-interrupt"), options: .atomic)
        try await wait { state.failure != nil || state.recovered }
        if let failure = state.failure { throw failure }
        guard state.timeline.items.isEmpty else { throw MobileError.invalidResponse }
        try await wait { state.failure != nil || state.timeline.items.first?.text == "恢复后" }
        if let failure = state.failure { throw failure }
        guard state.timeline.items.count == 1, !state.timeline.items.contains(where: { $0.text == "中" }) else { throw MobileError.invalidResponse }
        try Data().write(to: directory.appendingPathComponent("chat-revoke"), options: .atomic)
        try await wait { state.failure != nil }
        guard state.failure as? MobileError == .unauthorized, state.timeline.items.isEmpty else { throw MobileError.invalidResponse }
        await reader.value
        print("NATIVE_CHAT_RECOVERY_REVOKE_OK")
    }
    private static func wait(until condition: () -> Bool) async throws {
        // 15s = three backoffs (1+2+4) +5s receive deadline +3s scheduling margin.
        let deadline = ContinuousClock.now.advanced(by: .seconds(15))
        while !condition() {
            guard ContinuousClock.now < deadline else { throw MobileError.timedOut }
            try await Task.sleep(for: .milliseconds(50))
        }
    }
}
