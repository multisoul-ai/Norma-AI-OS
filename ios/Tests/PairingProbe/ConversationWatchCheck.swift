import Foundation
import NormaCore

/// Native probe only: observe the real Provider stream while the normal client sends two turns.
@MainActor final class ConversationWatchCheck {
    private(set) var timeline = ConversationTimeline()
    private(set) var assistantArrivals: [String: ContinuousClock.Instant] = [:]
    private(set) var received = 0
    private(set) var contentUpdates = 0
    private(set) var recoveries = 0
    private var ended = false
    private var failure: (any Error)?
    private var task: Task<Void, Never>?
    init(client: MobileClient, machine: PairedMachine, request: ConversationRequest) {
        task = Task {
            do {
                for try await update in client.conversationUpdates(machine, request: request) {
                    switch update {
                    case .current(let page):
                        received += 1
                        contentUpdates += 1
                        let arrived = ContinuousClock.now
                        timeline.merge(page, continuing: false)
                        for item in page.items where item.role == "assistant" {
                            let text = item.text.trimmingCharacters(in: .whitespacesAndNewlines)
                            if assistantArrivals[text] == nil { assistantArrivals[text] = arrived }
                        }
                    case .heartbeat: received += 1
                    case .recovering: recoveries += 1; timeline = ConversationTimeline()
                    }
                }
            } catch { if !Task.isCancelled { failure = error } }
            ended = true
        }
    }
    func requireActive() throws {
        if let failure { throw failure }
        guard !ended, recoveries == 0, received > 0 else { throw MobileError.invalidResponse }
    }
    func require(_ text: String) async throws {
        // 15s =7s reconnect backoff +5s receive bound +3s model-history scheduling allowance.
        let deadline = ContinuousClock.now.advanced(by: .seconds(15))
        while !timeline.items.contains(where: { $0.role == "assistant" && $0.text.trimmingCharacters(in: .whitespacesAndNewlines) == text }) {
            if let failure { throw failure }
            guard ContinuousClock.now < deadline else { throw MobileError.timedOut }
            try await Task.sleep(for: .milliseconds(50))
        }
    }
    func cancel() { task?.cancel() }
    func finish() async { task?.cancel(); await task?.value; task = nil }
}
