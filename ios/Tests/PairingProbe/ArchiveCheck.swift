import Foundation
import NormaCore

enum ArchiveCheck {
    static func run(client: MobileClient, machine: PairedMachine) async throws {
        let snapshot = try await client.snapshot(machine)
        guard let project = snapshot.projects.first, let route = project.routeID else { throw MobileError.invalidResponse }
        let current = try await client.snapshot(machine, workspaceID: project.workspaceID)
        guard current.projects.first?.cards?.isEmpty == true else { throw MobileError.invalidResponse }
        let scope = ProjectReviewRequest(workspaceID: project.workspaceID, routeID: route)
        let archive = try await client.sessionArchive(machine, scope: scope)
        guard archive.items.count == 1, archive.nextCursor == nil, let session = archive.items.first,
              session.title == "修复登录", session.key.profile == "Claude" else { throw MobileError.invalidResponse }
        let request = session.conversationRequest(scope: scope, cursor: nil)
        let result = try await client.archivedConversation(machine, request: request, profile: session.key.profile)
        guard result.page.items.count == 2, result.page.items[0].text == String(repeating: "完整消息", count: 1750),
              result.page.items[0].id == "reply-a", result.page.items[1].text == String(repeating: "归", count: 200_000),
              result.page.items[1].id == "reply-b", !result.page.hasMore else { throw MobileError.invalidResponse }
        do {
            _ = try await client.conversation(machine, request: request)
            throw MobileError.invalidResponse
        } catch MobileError.sessionChanged { }
        let wrong = ConversationRequest(workspaceID: scope.workspaceID, routeID: route, nodeID: session.key.nodeID,
                                        runID: session.key.runID, sessionID: "another-session", cursor: nil)
        do {
            _ = try await client.archivedConversation(machine, request: wrong, profile: "Claude")
            throw MobileError.invalidResponse
        } catch MobileError.historyUnavailable { }
        print("NATIVE_REAL_ARCHIVE_OK")
    }
}
