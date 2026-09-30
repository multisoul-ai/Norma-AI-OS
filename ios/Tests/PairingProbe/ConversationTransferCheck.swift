import Foundation
import NormaCore

enum ConversationTransferCheck {
    static func run(client: MobileClient, machine: PairedMachine, directory: URL, measuring: Bool = false) async throws {
        let catalog = try await client.snapshot(machine)
        guard let workspace = catalog.projects.first?.id else { throw MobileError.invalidResponse }
        let snapshot = try await client.snapshot(machine, workspaceID: workspace)
        guard let project = snapshot.projects.first, let route = project.routeID, let card = project.cards?.first,
              let run = card.runID, let session = card.sessionID else { throw MobileError.invalidResponse }
        let request = ConversationRequest(workspaceID: workspace, routeID: route, nodeID: card.id, runID: run, sessionID: session, cursor: nil)
        let response = try await client.conversation(machine, request: request)
        guard response.page.items.count == 2, response.page.items[0].text == String(repeating: "长", count: 200_000),
              let details = response.page.items[1].details,
              let tool = try JSONSerialization.jsonObject(with: Data(details.utf8)) as? [String: Any],
              tool["aggregatedOutput"] as? String == String(repeating: "\\\"\n", count: 200_000) else { throw MobileError.invalidResponse }
        if measuring {
            #if DEBUG
            try await NativeTransferTiming.run(client: client, machine: machine, request: request, expected: response)
            try await NativeTransferTiming.idle(client: client, machine: machine, request: request, expected: response, directory: directory)
            #else
            // Measurement uses testable transport access; the ordinary release probe still works.
            throw MobileError.unavailable
            #endif
        }
        var received = false
        for try await update in client.conversationUpdates(machine, request: request) {
            guard case .current(let page) = update else { throw MobileError.invalidResponse }
            guard page.items == response.page.items else { throw MobileError.invalidResponse }
            received = true; break // Dropping the unfolding sequence must close its socket owner.
        }
        guard received else { throw MobileError.invalidResponse }
        print("NATIVE_LARGE_CHAT_EVENT_OK")
        let stale = ConversationRequest(workspaceID: workspace, routeID: route, nodeID: card.id, runID: "old-run", sessionID: session, cursor: nil)
        do { _ = try await client.conversation(machine, request: stale); throw MobileError.invalidResponse }
        catch MobileError.sessionChanged {}
        print("NATIVE_LARGE_CONVERSATION_OK")
    }
}
