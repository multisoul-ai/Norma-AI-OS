import Foundation

/// Untrusted routing hint only. It carries no endpoint, credential or executable action.
public struct PushNotificationTarget: Decodable, Sendable, Identifiable, Hashable {
    public enum Kind: String, Decodable, Sendable { case approval, completed, failed }
    public let version: Int
    public let eventID: String
    public let kind: Kind
    public let machineID: String
    public let workspaceID: String
    public let routeID: String
    public let nodeID: String
    public let runID: String
    public let sessionID: String
    public let turnID: String
    public let requestID: String?
    public var id: String { eventID }
    enum CodingKeys: String, CodingKey, CaseIterable {
        case version, eventID = "event_id", kind, machineID = "machine_id", workspaceID = "workspace_id",
             routeID = "route_id", nodeID = "node_id", runID = "run_id", sessionID = "session_id", turnID = "turn_id", requestID = "request_id"
    }
    public static func decode(_ data: Data) throws -> Self {
        guard data.count <= 4096, let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys).isSubset(of: Set(CodingKeys.allCases.map(\.rawValue))) else { throw MobileError.invalidResponse }
        let target = try JSONDecoder().decode(Self.self, from: data)
        guard target.version == 1, UUID(uuidString: target.eventID) != nil,
              target.machineID.hasPrefix("mac-"), validHex(String(target.machineID.dropFirst(4))),
              [target.workspaceID, target.routeID, target.runID, target.sessionID, target.turnID].allSatisfy(validID),
              let node = UInt64(target.nodeID), node > 0, String(node) == target.nodeID else { throw MobileError.invalidResponse }
        if target.kind == .approval {
            guard let request = target.requestID, !request.isEmpty, request.utf8.count <= 256,
                  request.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || [45, 95, 58].contains($0) }) else { throw MobileError.invalidResponse }
        } else if target.requestID != nil { throw MobileError.invalidResponse }
        return target
    }
    public func card(in project: ProjectSnapshot) throws -> CardSnapshot {
        guard project.id == workspaceID, project.state == "open", project.routeID == routeID,
              let card = project.cards?.first(where: { String($0.id) == nodeID }),
              card.runID == runID, card.sessionID == sessionID, card.turnID == turnID else { throw MobileError.sessionChanged }
        return card
    }
    public func matches(_ approval: ApprovalSnapshot) -> Bool {
        let t = approval.target
        return kind == .approval && String(t.nodeID) == nodeID && t.runID == runID
            && t.sessionID == sessionID && t.turnID == turnID && t.requestID == requestID
    }
}
