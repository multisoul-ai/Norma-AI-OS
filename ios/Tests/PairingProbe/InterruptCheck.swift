import Foundation
import NormaCore

enum InterruptCheck {
    static func run(client: MobileClient, machine: PairedMachine) async throws {
        let catalog = try await client.snapshot(machine)
        guard catalog.projects.count == 1 else { throw MobileError.invalidResponse }
        let workspace = catalog.projects[0].id
        let snapshot = try await client.snapshot(machine, workspaceID: workspace)
        guard let project = snapshot.projects.first, let route = project.routeID,
              let card = project.cards?.first, card.isBusy else { throw MobileError.invalidResponse }
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("stop.json")
        let pending = try await client.prepareInterrupt(machine, workspaceID: workspace, routeID: route, revision: snapshot.grant.revision, card: card)
        try await PendingCommandStore(url: url).save(pending)
        let started = ContinuousClock.now
        _ = try await client.submit(pending, to: machine) // Lose the initial response; recover by ID.
        let restored = try await PendingCommandStore(url: url).load()
        guard restored == [pending] else { throw MobileError.invalidResponse }
        let stopped = try await MessageCheck.receipt(client, machine, restored[0])
        let elapsed = started.duration(to: .now).components
        print("INTERRUPT_LATENCY_MS \(Double(elapsed.seconds) * 1000 + Double(elapsed.attoseconds) / 1e15)")
        print("INTERRUPT_RECEIPT \(card.profile) \(stopped.state) \(stopped.result?.status ?? stopped.error ?? "none")")
        guard stopped.state == "succeeded", stopped.result?.status == "interrupted",
              stopped.result?.turnID == card.turnID else { throw MobileError.invalidResponse }
        let repeated = try await client.submit(pending, to: machine)
        guard repeated.result?.turnID == card.turnID, repeated.result?.status == "interrupted" else { throw MobileError.invalidResponse }
        let latest = try await client.snapshot(machine, workspaceID: workspace)
        guard let idle = latest.projects.first?.cards?.first, !idle.isBusy,
              idle.runID == card.runID, idle.sessionID == card.sessionID else { throw MobileError.invalidResponse }
        let next = try await client.prepareMessage(machine, workspaceID: workspace, routeID: route, revision: latest.grant.revision, card: idle,
                                                  text: "Do not use any tools, read or modify files. Reply exactly NORMA_AFTER_INTERRUPT")
        try await PendingCommandStore(url: url).save(next)
        _ = try await client.submit(next, to: machine)
        let sent = try await MessageCheck.receipt(client, machine, next)
        print("INTERRUPT_NEXT_RECEIPT \(sent.state) \(sent.error ?? "none")")
        guard sent.state == "succeeded", sent.result?.sessionID == card.sessionID, sent.result?.turnID != card.turnID else { throw MobileError.invalidResponse }
        // Retry the old stop while the new message exists; it must return its original receipt.
        let old = try await client.submit(pending, to: machine)
        guard old.result?.turnID == card.turnID else { throw MobileError.invalidResponse }
        _ = try await MessageCheck.completed(client, machine, workspace, route, card.id, "NORMA_AFTER_INTERRUPT")
        print("NATIVE_REAL_INTERRUPTS_OK \(card.profile)")
    }
}
