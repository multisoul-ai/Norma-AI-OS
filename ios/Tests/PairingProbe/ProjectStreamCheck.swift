import Foundation
import NormaCore

/// Isolated native client exercises the production stream, command receipts and read-only recovery.
@MainActor enum ProjectStreamCheck {
    private final class Observation {
        var latest: MachineSnapshot?
        var received = 0
        var recovering = 0
        var failure: (any Error)?
    }
    static func run(client: MobileClient, machine: PairedMachine, directory: URL) async throws {
        let catalog = try await client.snapshot(machine)
        guard let workspace = catalog.projects.first?.id else { throw MobileError.invalidResponse }
        let state = Observation()
        let approvals = ApprovalWatchCheck(client: client, machine: machine)
        defer { approvals.cancel() }
        let reader = Task {
            do {
                for try await update in client.projectUpdates(machine, workspaceID: workspace) {
                    switch update {
                    case .current(let snapshot):
                        state.latest = snapshot; state.received += 1
                        //4.5s=two2s heartbeats+0.5s margin. A busy UI must resume at a complete latest state,
                        //not fail because intermediate, already-validated snapshots filled a one-item buffer.
                        if state.received == 1 { try await Task.sleep(for: .milliseconds(4500)) }
                    case .recovering: state.latest = nil; state.recovering += 1
                    }
                }
            } catch { state.latest = nil; state.failure = error }
        }
        defer { reader.cancel() }
        try await wait(state) { state.latest != nil }
        guard let first = state.latest, let project = first.projects.first, let route = project.routeID,
              project.cards?.count == 1, let retained = project.cards?.first else { throw MobileError.invalidResponse }
        let approvalScope = ApprovalListRequest(workspaceID: workspace, routeID: route)
        try await approvals.require(approvalScope, revision: first.grant.revision)
        guard try approvals.items(approvalScope)?.isEmpty == true else { throw MobileError.invalidResponse }
        // 6s exceeds the ordinary request's 5s lifetime; heartbeat must keep this same watch alive.
        let start = ContinuousClock.now
        try await wait(state) { ContinuousClock.now - start >= .seconds(6) && state.received >= 4 }
        guard state.recovering == 0 else { throw MobileError.invalidResponse }
        print("NATIVE_STREAM_HEARTBEAT_OK")
        let command = try await client.prepareCardCreation(machine, workspaceID: workspace, routeID: route,
            revision: first.grant.revision, profile: "Claude", title: "Stream-created")
        _ = try await client.submit(command, to: machine)
        let receipt = try await MessageCheck.receipt(client, machine, command)
        guard receipt.state == "succeeded", let node = receipt.result?.nodeID else { throw MobileError.invalidResponse }
        try await wait(state) { state.latest?.projects.first?.cards?.contains { $0.id == node } == true }
        guard let created = state.latest?.projects.first?.cards?.first(where: { $0.id == node }),
              created.state == "idle", created.runID == nil,
              state.latest?.projects.first?.cards?.contains(where: { $0.id == retained.id }) == true else { throw MobileError.invalidResponse }
        // A local filter must use each new authenticated snapshot, without fetching or caching another list.
        guard state.latest?.projects.first?.cards?.filter({ $0.matchesSearch("claude stream", state: "idle", profile: "Claude") }).map(\.id) == [node],
              !retained.matchesSearch("claude stream", state: "idle", profile: "Claude"),
              !created.matchesSearch("stream", state: "thinking") else { throw MobileError.invalidResponse }
        let deletion = try await client.prepareCardDeletion(machine, workspaceID: workspace, routeID: route,
            revision: first.grant.revision, card: created, confirmBusy: false)
        _ = try await client.submit(deletion, to: machine)
        let deleted = try await MessageCheck.receipt(client, machine, deletion)
        guard deleted.state == "succeeded", deleted.result?.nodeID == node else { throw MobileError.invalidResponse }
        try await wait(state) { state.latest?.projects.first?.cards?.contains { $0.id == node } == false }
        guard state.latest?.projects.first?.cards?.map(\.id) == [retained.id] else { throw MobileError.invalidResponse }
        guard state.latest?.projects.first?.cards?.filter({ $0.matchesSearch("claude stream", state: "idle", profile: "Claude") }).isEmpty == true,
              state.latest?.projects.first?.cards?.filter({ $0.matchesSearch("") }).map(\.id) == [retained.id] else { throw MobileError.invalidResponse }
        print("NATIVE_STREAM_SEARCH_DELTA_OK")
        print("NATIVE_STREAM_CARD_DELTA_OK")
        // Test-only marker asks the Rust owner to downgrade this exact phone. App has no grant-writing API.
        try Data().write(to: directory.appendingPathComponent("stream-read-only"), options: .atomic)
        try await wait(state) { state.latest?.grant.revision == first.grant.revision + 1 }
        guard state.recovering > 0, state.latest?.grant.projects.first?.access == "read",
              state.latest?.projects.first?.cards?.map(\.id) == [retained.id] else { throw MobileError.invalidResponse }
        try await approvals.require(approvalScope, revision: first.grant.revision + 1)
        guard approvals.recoveries > 0, approvals.latest?.grant.projects.first?.access == "read" else { throw MobileError.invalidResponse }
        print("NATIVE_APPROVAL_FEED_DOWNGRADE_OK")
        let denied = try await client.prepareCardCreation(machine, workspaceID: workspace, routeID: route,
            revision: first.grant.revision + 1, profile: "Codex", title: "Never-created")
        do { _ = try await client.submit(denied, to: machine); throw MobileError.invalidResponse }
        catch MobileError.unauthorized {}
        reader.cancel(); await reader.value
        let count = state.received
        await approvals.finish()
        let approvalCount = approvals.received
        // Two heartbeat periods (2 × 2s) prove cancellation cannot keep delivering to the old consumer.
        try await Task.sleep(for: .seconds(4))
        guard state.received == count, approvals.received == approvalCount else { throw MobileError.invalidResponse }
        print("NATIVE_APPROVAL_FEED_CANCEL_OK")
        print("NATIVE_REAL_PROJECT_EVENTS_OK")
    }
    private static func wait(_ state: Observation, until condition: () -> Bool) async throws {
        // 15s = three reconnect backoffs (1+2+4) plus 5s network bound and 3s scheduling allowance.
        let deadline = ContinuousClock.now.advanced(by: .seconds(15))
        while !condition() {
            if let error = state.failure { throw error }
            guard ContinuousClock.now < deadline else { throw MobileError.timedOut }
            try await Task.sleep(for: .milliseconds(50))
        }
    }
}
