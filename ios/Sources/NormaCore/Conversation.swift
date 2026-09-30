import Foundation

public struct ConversationRequest: Codable, Sendable, Equatable {
    public let workspaceID: String
    public let routeID: String
    public let nodeID: UInt64
    public let runID: String
    public let sessionID: String
    public var cursor: String?
    public init(workspaceID: String, routeID: String, nodeID: UInt64, runID: String, sessionID: String, cursor: String?) {
        self.workspaceID = workspaceID; self.routeID = routeID; self.nodeID = nodeID
        self.runID = runID; self.sessionID = sessionID; self.cursor = cursor
    }
    enum CodingKeys: String, CodingKey {
        case workspaceID = "workspace_id", routeID = "route_id", nodeID = "node_id", runID = "run_id", sessionID = "session_id", cursor
    }
    func validate() throws {
        guard nodeID > 0, [workspaceID, routeID, runID, sessionID].allSatisfy(validID), (cursor?.utf8.count ?? 0) <= 4096 else { throw MobileError.invalidResponse }
    }
}

public struct ConversationItem: Decodable, Sendable, Identifiable, Equatable {
    public let id: String
    public let role: String
    public let text: String
    public let details: String?
    public let detailCards: [ConversationDetailCard]

    init(id: String, role: String, text: String, details: String?) {
        self.id = id; self.role = role; self.text = text; self.details = details
        detailCards = details.map(ConversationDetailCard.parse) ?? []
    }
    enum CodingKeys: String, CodingKey { case id, role, text, details }
    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        self.init(id: try values.decode(String.self, forKey: .id), role: try values.decode(String.self, forKey: .role),
                  text: try values.decode(String.self, forKey: .text), details: try values.decodeIfPresent(String.self, forKey: .details))
    }
}
public struct ConversationPage: Decodable, Sendable {
    public let items: [ConversationItem]
    public let cursor: String?
    public let hasMore: Bool
    public let pendingTail: Bool
    public let direction: String
    enum CodingKeys: String, CodingKey { case items, cursor, hasMore = "has_more", pendingTail = "pending_tail", direction }
}
public struct ConversationResponse: Decodable, Sendable {
    public let request: ConversationRequest
    public let page: ConversationPage
    func validate(for request: ConversationRequest) throws {
        try request.validate()
        guard self.request == request, ["older", "newer"].contains(page.direction),
              Set(page.items.map(\.id)).count == page.items.count,
              page.items.allSatisfy({ !$0.id.isEmpty && $0.id.utf8.count <= 512 && ["user", "assistant", "tool"].contains($0.role) }),
              (page.cursor?.utf8.count ?? 0) <= 4096,
              !page.hasMore || page.cursor != nil,
              page.direction != "newer" || page.cursor != nil else { throw MobileError.invalidResponse }
    }
}

public struct ConversationTimeline: Sendable {
    public private(set) var items: [ConversationItem] = []
    public private(set) var cursor: String?
    public private(set) var hasMore = false
    public init() { }
    /// Returns true when a missing range requires rebuilding the visible history from this page.
    @discardableResult public mutating func merge(_ page: ConversationPage, continuing: Bool) -> Bool {
        let first = items.isEmpty
        let ids = Set(items.map(\.id))
        let incomingIDs = Set(page.items.map(\.id))
        let gap = page.direction == "older" && !continuing && !first && !page.items.isEmpty && ids.isDisjoint(with: incomingIDs) && page.hasMore
        if page.direction == "newer" {
            let replacements = Dictionary(uniqueKeysWithValues: page.items.map { ($0.id, $0) })
            items = items.map { replacements[$0.id] ?? $0 } + page.items.filter { !ids.contains($0.id) }
        } else if continuing {
            items = page.items.filter { !ids.contains($0.id) } + items
        } else if !page.hasMore || gap || first {
            items = page.items
        } else if let overlap = items.firstIndex(where: { incomingIDs.contains($0.id) }) {
            items = Array(items[..<overlap]) + page.items
        }
        if page.direction == "newer" || continuing || first || gap || !page.hasMore {
            cursor = page.cursor; hasMore = page.hasMore
        }
        return gap
    }
}

extension MobileClient {
    public func conversation(_ machine: PairedMachine, request: ConversationRequest) async throws -> ConversationResponse {
        try await readConversation(machine, request: request, archivedProfile: nil)
    }
    public func archivedConversation(_ machine: PairedMachine, request: ConversationRequest, profile: String) async throws -> ConversationResponse {
        guard MobileAgentProfile.supports(profile), request.nodeID <= Int64.max else { throw MobileError.invalidResponse }
        return try await readConversation(machine, request: request, archivedProfile: profile)
    }
    private func readConversation(_ machine: PairedMachine, request: ConversationRequest, archivedProfile: String?) async throws -> ConversationResponse {
        try machine.validate(); try request.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        let requestJSON = try JSONSerialization.jsonObject(with: JSONEncoder().encode(request))
        let id = UUID().uuidString
        var action: [String: Any] = ["type": archivedProfile == nil ? "conversation" : "archived_conversation", "device_id": credential.deviceID, "token": credential.token, "request": requestJSON]
        if let archivedProfile { action["profile"] = archivedProfile }
        let body = try JSONSerialization.data(withJSONObject: ["version": 1, "request_id": id, "machine_id": machine.id, "action": action])
        let connection = try await PinnedConnection.open(machine, streaming: true)
        defer { connection.close() }
        // Archive12s host/read budget +3s transport/scheduling; live reads retain5s.
        var data = try await connection.exchange(body, timeout: .seconds(archivedProfile == nil ? 5 : 15))
        var transfer = ConversationTransfer()
        var fragmented = false
        let conversation: ConversationResponse
        while true {
            let reply = try WireReply.decode(data, expectedID: id)
            if !fragmented, reply.result?.status == "conversation", let result = reply.result?.conversation {
                conversation = result; break
            }
            guard reply.result?.status == "conversation_fragment", let fragment = reply.result?.fragment else { throw MobileError.invalidResponse }
            fragmented = true
            if let result = try transfer.append(fragment, as: ConversationResponse.self) { conversation = result; break }
            data = try await connection.receive()
        }
        try conversation.validate(for: request)
        return conversation
    }
}
