import Foundation

public struct ProjectReviewRequest: Codable, Sendable, Equatable {
    public let workspaceID: String
    public let routeID: String
    public let deliveryKey: MobileTurnOutcome.Key?
    public init(workspaceID: String, routeID: String, deliveryKey: MobileTurnOutcome.Key? = nil) { self.workspaceID = workspaceID; self.routeID = routeID; self.deliveryKey = deliveryKey }
    enum CodingKeys: String, CodingKey { case workspaceID = "workspace_id", routeID = "route_id", deliveryKey = "delivery_key" }
    func validate() throws {
        try deliveryKey?.validate()
        guard validID(workspaceID), validID(routeID) else { throw MobileError.invalidResponse }
    }
}
public struct ReviewFileVersion: Decodable, Sendable {
    public let kind: String
    public let executable: Bool
    public let bytes: [UInt8]
    public var text: String? { kind == "binary" ? nil : String(bytes: bytes, encoding: .utf8) }
}
public struct ReviewFileChange: Decodable, Sendable, Identifiable {
    public let path: String
    public let status: String
    public let before: ReviewFileVersion?
    public let after: ReviewFileVersion?
    public let index: ReviewFileVersion?
    public let diff: String?
    public let stagedDiff: String?
    public var id: String { path }
    public var isConflicted: Bool { status.contains("U") || ["AA", "DD"].contains(status) }
    public var changeLabel: String {
        if isConflicted { return "冲突" }
        if after == nil { return "删除" }
        if before == nil { return "新增" }
        return "修改"
    }
    enum CodingKeys: String, CodingKey { case path, status, before, after, index, diff, stagedDiff = "staged_diff" }
}
public struct ProjectReview: Decodable, Sendable {
    public let baseCommit: String?
    public let files: [ReviewFileChange]
    public let conflicted: Bool
    enum CodingKeys: String, CodingKey { case baseCommit = "base_commit", files, conflicted }
}
public struct ProjectReviewResponse: Decodable, Sendable {
    public let request: ProjectReviewRequest
    public let review: ProjectReview
    public let delivery: TaskDeliveryReference?
    func validate(for expected: ProjectReviewRequest) throws {
        try expected.validate()
        if let key = expected.deliveryKey {
            guard let delivery, delivery.outcome.key == key else { throw MobileError.invalidResponse }
            try delivery.validate()
            if let acceptance = delivery.acceptance {
                guard acceptance.workspaceID == expected.workspaceID, !review.conflicted else { throw MobileError.invalidResponse }
            }
        } else if delivery != nil { throw MobileError.invalidResponse }
        guard request == expected, review.files.count <= 128,
              Set(review.files.map(\.path)).count == review.files.count,
              review.conflicted == review.files.contains(where: \.isConflicted) else { throw MobileError.invalidResponse }
        if let hash = review.baseCommit {
            guard [40, 64].contains(hash.utf8.count), hash.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else { throw MobileError.invalidResponse }
        }
        var total = 0
        for file in review.files {
            guard !file.path.isEmpty, file.path.utf8.count <= 4096, !file.path.contains("\0"),
                  file.path.split(separator: "/", omittingEmptySubsequences: false).allSatisfy({ !$0.isEmpty && ![".", "..", ".git", ".norma", ".multisoul"].contains($0.lowercased()) }),
                  file.status.utf8.count == 2, file.status.allSatisfy({ " MADTU?!".contains($0) }) else { throw MobileError.invalidResponse }
            for value in [file.before, file.after, file.index].compactMap({ $0 }) {
                guard ["text", "binary", "symlink"].contains(value.kind), value.bytes.count <= 1024 * 1024,
                      value.kind != "text" || (value.text != nil && !value.bytes.contains(0)),
                      value.kind != "symlink" || !value.executable else { throw MobileError.invalidResponse }
                total += value.bytes.count
            }
            total += (file.diff?.utf8.count ?? 0) + (file.stagedDiff?.utf8.count ?? 0)
        }
        guard total <= 8 * 1024 * 1024 else { throw MobileError.reviewTooLarge }
    }
}

extension MobileClient {
    public func projectReview(_ machine: PairedMachine, request: ProjectReviewRequest) async throws -> ProjectReviewResponse {
        try machine.validate(); try request.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        let target = try JSONSerialization.jsonObject(with: JSONEncoder().encode(request))
        let id = UUID().uuidString
        let body = try JSONSerialization.data(withJSONObject: ["version": 1, "request_id": id, "machine_id": machine.id,
            "action": ["type": "project_review", "device_id": credential.deviceID, "token": credential.token, "request": target]])
        let connection = try await PinnedConnection.open(machine, streaming: true)
        defer { connection.close() }
        // Route checks2s×2 + capture10s + delivery/scheduling4s =18s for the first frame only.
        var data = try await connection.exchange(body, timeout: .seconds(18))
        var transfer = ConversationTransfer()
        var fragmented = false
        let response: ProjectReviewResponse
        while true {
            try Task.checkCancellation()
            let reply = try WireReply.decode(data, expectedID: id)
            if !fragmented, reply.result?.status == "project_review", let result = reply.result?.review { response = result; break }
            guard reply.result?.status == "review_fragment", let part = reply.result?.fragment else { throw MobileError.invalidResponse }
            fragmented = true
            if let complete = try transfer.append(part, as: ProjectReviewResponse.self) { response = complete; break }
            data = try await connection.receive()
        }
        try response.validate(for: request)
        return response
    }
}
