import Foundation

public struct ArchivedSession: Decodable, Sendable, Identifiable {
    public struct Key: Decodable, Sendable {
        public let nodeID: UInt64
        public let profile: String
        public let runID: String
        public let sessionID: String
        enum CodingKeys: String, CodingKey { case nodeID = "node_id", profile, runID = "run_id", sessionID = "session_id" }
    }
    public let key: Key
    public let sequence: UInt64
    public let recordedAtUnix: UInt64
    public let title: String
    public var id: String { key.runID }
    enum CodingKeys: String, CodingKey { case key, sequence, recordedAtUnix = "recorded_at_unix", title }
    public func conversationRequest(scope: ProjectReviewRequest, cursor: String?) -> ConversationRequest {
        ConversationRequest(workspaceID: scope.workspaceID, routeID: scope.routeID, nodeID: key.nodeID, runID: key.runID, sessionID: key.sessionID, cursor: cursor)
    }
}
public struct SessionArchivePage: Decodable, Sendable {
    public let request: ProjectReviewRequest
    public let items: [ArchivedSession]
    public let nextCursor: UInt64?
    enum CodingKeys: String, CodingKey { case request, items, nextCursor = "next_cursor" }
    func validate(for scope: ProjectReviewRequest, before: UInt64?) throws {
        try scope.validate()
        guard request == scope, scope.deliveryKey == nil, items.count <= 20,
              before.map({ $0 > 0 && $0 <= Int64.max }) ?? true,
              Set(items.map(\.id)).count == items.count else { throw MobileError.invalidResponse }
        var previous = before ?? UInt64.max
        for item in items {
            try item.conversationRequest(scope: scope, cursor: nil).validate()
            guard item.key.nodeID <= Int64.max, ["Codex", "Claude"].contains(item.key.profile),
                  item.sequence > 0, item.sequence <= Int64.max, item.sequence < previous,
                  item.recordedAtUnix > 0, item.recordedAtUnix <= Int64.max,
                  !item.title.isEmpty, item.title.utf8.count <= 4096 else { throw MobileError.invalidResponse }
            previous = item.sequence
        }
        guard nextCursor == (items.count == 20 ? items.last?.sequence : nil) else { throw MobileError.invalidResponse }
    }
}
extension MobileClient {
    public func sessionArchive(_ machine: PairedMachine, scope: ProjectReviewRequest, before: UInt64? = nil) async throws -> SessionArchivePage {
        try machine.validate(); try scope.validate()
        guard scope.deliveryKey == nil, before.map({ $0 > 0 && $0 <= Int64.max }) ?? true else { throw MobileError.invalidResponse }
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        var action: [String: Any] = ["type": "session_archive", "device_id": credential.deviceID, "token": credential.token,
            "request": try JSONSerialization.jsonObject(with: JSONEncoder().encode(scope))]
        if let before { action["before"] = before }
        let reply = try await exchange(machine, action: action, timeout: .seconds(10))
        guard reply.result?.status == "session_archive", let page = reply.result?.archive else { throw MobileError.invalidResponse }
        try page.validate(for: scope, before: before)
        return page
    }
}
