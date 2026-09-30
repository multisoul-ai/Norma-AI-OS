import Foundation
import NormaCore

enum CommandCheck {
    static func run(client: MobileClient, machine: PairedMachine, alternate: PairedMachine, detail: MachineSnapshot, directory: URL) async throws -> PendingMobileCommand {
        let project = detail.projects[0]
        guard let route = project.routeID else { throw MobileError.invalidResponse }
        let denied = try await client.prepareCardCreation(machine, workspaceID: project.id, routeID: route, revision: 1, profile: "Codex", title: "不可创建")
        do { _ = try await client.submit(denied, to: machine); throw MobileError.invalidResponse }
        catch MobileError.unauthorized { }
        // File marker is consumed only by the isolated Rust fixture; the app has no grant endpoint.
        try Data("control".utf8).write(to: directory.appendingPathComponent("enable-control"), options: .atomic)
        let deadline = ContinuousClock.now.advanced(by: .seconds(30))
        var grant: DeviceGrant?
        while ContinuousClock.now < deadline {
            let current = try await client.authenticate(machine)
            if current.revision == 2, current.projects.first?.access == "control" { grant = current; break }
            try await Task.sleep(for: .milliseconds(50))
        }
        guard let grant else { throw MobileError.timedOut }
        let command = try await client.prepareCardCreation(machine, workspaceID: project.id, routeID: route, revision: grant.revision, profile: "Codex", title: "Swift 创建")
        let storeURL = directory.appendingPathComponent("pending-command.json")
        try await PendingCommandStore(url: storeURL).save(command)
        _ = try await client.submit(command, to: machine)
        let persisted = try await PendingCommandStore(url: storeURL).load()
        guard persisted == [command] else { throw MobileError.invalidResponse }
        // Deliberately discard the initial receipt, reload its identity, then query via the other listener.
        var receipt = try await client.receipt(for: persisted[0], on: alternate)
        while receipt.state == "accepted", ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(50))
            receipt = try await client.receipt(for: persisted[0], on: alternate)
        }
        guard receipt.state == "succeeded", let node = receipt.result?.nodeID else { throw MobileError.invalidResponse }
        let replay = try await client.submit(command, to: machine)
        guard replay.result?.nodeID == node else { throw MobileError.invalidResponse }
        let changed = try PendingMobileCommand(machineID: command.machineID, deviceID: command.deviceID, workspaceID: project.id, routeID: route, revision: 2, profile: "Codex", title: "不可重复", operationID: command.request.operationID)
        do { _ = try await client.submit(changed, to: machine); throw MobileError.invalidResponse }
        catch MobileError.commandConflict { }
        let current = try await client.snapshot(machine, workspaceID: project.id)
        guard let cards = current.projects[0].cards, cards.count == 3, cards.last?.nodeID == node,
              cards.last?.title == "Swift 创建", cards.last?.state == "idle" else { throw MobileError.invalidResponse }
        try await PendingCommandStore(url: storeURL).remove(id: command.id)
        print("NATIVE_CREATE_RECEIPT_OK")
        guard let target = cards.last else { throw MobileError.invalidResponse }
        let deletion = try await client.prepareCardDeletion(machine, workspaceID: project.id, routeID: route, revision: 2, card: target, confirmBusy: false)
        try await PendingCommandStore(url: storeURL).save(deletion)
        _ = try await client.submit(deletion, to: machine)
        let reloaded = try await PendingCommandStore(url: storeURL).load()
        guard reloaded == [deletion] else { throw MobileError.invalidResponse }
        var deleted = try await client.receipt(for: reloaded[0], on: alternate)
        while deleted.state == "accepted", ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(50))
            deleted = try await client.receipt(for: deletion, on: alternate)
        }
        guard deleted.state == "succeeded", deleted.result?.nodeID == node else { throw MobileError.invalidResponse }
        let repeatedDelete = try await client.submit(deletion, to: machine)
        guard repeatedDelete.state == "succeeded", repeatedDelete.result?.nodeID == node else { throw MobileError.invalidResponse }
        let remaining = try await client.snapshot(machine, workspaceID: project.id)
        guard let retained = remaining.projects[0].cards, retained.count == 2,
              !retained.contains(where: { $0.nodeID == node }), retained.contains(where: { $0.profile == "Claude" }) else { throw MobileError.invalidResponse }
        try await PendingCommandStore(url: storeURL).remove(id: deletion.id)
        print("NATIVE_DELETE_RECEIPT_OK")
        return command
    }
}
