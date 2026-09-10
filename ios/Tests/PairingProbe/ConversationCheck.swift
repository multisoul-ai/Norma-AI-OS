import Foundation
import NormaCore

enum ConversationCheck {
    static func run(client: MobileClient, machine: PairedMachine) async throws {
        let catalog = try await client.snapshot(machine)
        guard catalog.projects.count == 1 else { throw MobileError.invalidResponse }
        let detail = try await client.snapshot(machine, workspaceID: catalog.projects[0].id)
        guard let project = detail.projects.first, let route = project.routeID, let cards = project.cards,
              cards.count == 1, let card = cards.first, let run = card.runID, let session = card.sessionID else { throw MobileError.invalidResponse }
        let request = ConversationRequest(workspaceID: project.id, routeID: route, nodeID: card.id, runID: run, sessionID: session, cursor: nil)
        let page = try await client.conversation(machine, request: request).page
        let expected = card.profile == "Codex" ? "NORMA_CODEX_APP_READY" : "NORMA_CLAUDE_CHAT_READY"
        guard page.items.contains(where: { $0.role == "assistant" && $0.text.trimmingCharacters(in: .whitespacesAndNewlines) == expected }) else { throw MobileError.invalidResponse }
        let foreign = ConversationRequest(workspaceID: project.id, routeID: route, nodeID: card.id, runID: "old-run", sessionID: session, cursor: nil)
        do { _ = try await client.conversation(machine, request: foreign); throw MobileError.invalidResponse }
        catch MobileError.sessionChanged { }
        print("NATIVE_REAL_CONVERSATION_OK \(card.profile)")
    }
}
