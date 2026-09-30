import Foundation

struct ThreadReviewRequest: Codable, Equatable, Sendable {
    let collaborationWorkspaceID: String
    var threadID: String
    let operationID: String
    let taskRef: String
    var fingerprint: String
    var offset: Int
    enum CodingKeys: String, CodingKey {
        case collaborationWorkspaceID = "collaboration_workspace_id", threadID = "thread_id",
             operationID = "operation_id", taskRef = "task_ref", fingerprint, offset
    }
}
struct ThreadReviewChunk: Decodable, Sendable {
    let request: ThreadReviewRequest
    let totalBytes: Int
    let data: String
    enum CodingKeys: String, CodingKey { case request, totalBytes = "total_bytes", data }
}

/// Keep partial bytes offscreen. The expected task and frozen delivery never follow a new turn.
struct ThreadReviewTransfer {
    private(set) var request: ThreadReviewRequest
    private let scope: ProjectReviewRequest
    private let delivery: TaskDeliverySummary
    private var total: Int?
    private var bytes = Data()
    private var ended = false

    init(request: ThreadReviewRequest, scope: ProjectReviewRequest, delivery: TaskDeliverySummary) throws {
        try scope.validate(); try delivery.validate(workspaceID: scope.workspaceID, nodeID: nil)
        guard request.offset == 0, request.fingerprint == delivery.fingerprint, delivery.available,
              scope.deliveryKey == delivery.outcome.key else { throw MobileError.invalidResponse }
        self.request = request; self.scope = scope; self.delivery = delivery
    }

    mutating func append(_ chunk: ThreadReviewChunk) throws -> ProjectReviewResponse? {
        do {
            guard !ended, chunk.request == request,
                  chunk.totalBytes > 0, chunk.totalBytes <= 32 * 1024 * 1024,
                  total == nil || total == chunk.totalBytes,
                  chunk.data.utf8.count <= 4 * ((160 * 1024 + 2) / 3),
                  let part = Data(base64Encoded: chunk.data), !part.isEmpty,
                  part.count <= 160 * 1024, part.base64EncodedString() == chunk.data,
                  part.count <= chunk.totalBytes - bytes.count else { throw MobileError.invalidResponse }
            total = chunk.totalBytes; bytes.append(part); request.offset = bytes.count
            guard bytes.count == chunk.totalBytes else { return nil }
            let response = try JSONDecoder().decode(ProjectReviewResponse.self, from: bytes)
            try response.validate(for: scope)
            guard response.delivery?.fingerprint == delivery.fingerprint,
                  response.delivery?.outcome == delivery.outcome,
                  response.delivery?.savedAtUnix == delivery.savedAtUnix,
                  response.review.files.count == delivery.fileCount else { throw MobileError.invalidResponse }
            ended = true; bytes.removeAll(keepingCapacity: false)
            return response
        } catch {
            ended = true; bytes.removeAll(keepingCapacity: false)
            throw error
        }
    }
}

extension CollaborationRelayConnection {
    public func threadTaskReview(task: ThreadTaskOperation, receipt: ThreadTaskReceipt) async throws -> ProjectReviewResponse {
        guard capabilities.contains("thread_task_review_v1") else { throw ThreadTaskError.resultsRequireUpgrade }
        guard let grant else { throw MobileError.unauthorized }
        try receipt.validate(task: task, machineID: machineID, deviceID: grant.deviceID)
        guard let delivery = receipt.execution?.execution.delivery, delivery.available,
              let taskRef = receipt.receipt.result?.taskRef else { throw MobileError.reviewUnavailable }
        var transfer = try ThreadReviewTransfer(request: .init(collaborationWorkspaceID: task.collaborationWorkspaceID,
            threadID: task.threadID, operationID: task.operationID, taskRef: taskRef,
            fingerprint: delivery.fingerprint, offset: 0),
            scope: .init(workspaceID: task.projectID, routeID: receipt.receipt.routeID, deliveryKey: delivery.outcome.key), delivery: delivery)
        do {
            while true {
                try Task.checkCancellation()
                let query = try JSONSerialization.jsonObject(with: JSONEncoder().encode(transfer.request))
                let reply = try await exchange(type: "thread_task_review", fields: ["request": query])
                guard reply.result?.status == "thread_task_review", let chunk = reply.result?.threadReview else { throw MobileError.invalidResponse }
                if let response = try transfer.append(chunk) { return response }
            }
        } catch { close(); throw error }
    }
}
