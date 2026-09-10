import Foundation
import NormaCore

/// Isolated native probe keeps one actual computer feed alive across both real approval turns.
@MainActor final class ApprovalWatchCheck {
    private(set) var latest: ApprovalFeedSnapshot?
    private(set) var received = 0
    private(set) var recoveries = 0
    private var failure: (any Error)?
    private var task: Task<Void, Never>?
    init(client: MobileClient, machine: PairedMachine) {
        task = Task {
            do {
                for try await update in client.approvalUpdates(machine) {
                    switch update {
                    case .current(let snapshot): latest = snapshot; received += 1
                    case .recovering: latest = nil; recoveries += 1
                    }
                }
            } catch { latest = nil; if !Task.isCancelled { failure = error } }
        }
    }
    func items(_ request: ApprovalListRequest) throws -> [ApprovalSnapshot]? {
        if let failure { throw failure }
        guard let latest else { return nil }
        guard let project = latest.projects.first(where: { $0.project.id == request.workspaceID && $0.project.routeID == request.routeID }) else { throw MobileError.sessionChanged }
        guard project.error == nil else { throw MobileError.approvalUnavailable }
        return project.items
    }
    func require(_ request: ApprovalListRequest, removed target: ApprovalTarget? = nil, revision: UInt64? = nil) async throws {
        //15s =7s read-only reconnect backoff +5s receive bound +3s scheduling margin.
        let deadline = ContinuousClock.now.advanced(by: .seconds(15))
        while true {
            if let items = try items(request), revision.map({ latest?.grant.revision == $0 }) ?? true,
               target.map({ old in !items.contains { $0.target == old } }) ?? true { return }
            guard ContinuousClock.now < deadline else { throw MobileError.timedOut }
            try await Task.sleep(for: .milliseconds(50))
        }
    }
    func cancel() { task?.cancel() }
    func finish() async { task?.cancel(); await task?.value; task = nil }
}
