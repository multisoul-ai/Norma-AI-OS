import Foundation
import NormaCore

enum ReviewCheck {
    static func run(client: MobileClient, machine: PairedMachine) async throws {
        let snapshot = try await client.snapshot(machine)
        guard let project = snapshot.projects.first, let route = project.routeID else { throw MobileError.invalidResponse }
        let request = ProjectReviewRequest(workspaceID: project.workspaceID, routeID: route)
        let response = try await client.projectReview(machine, request: request)
        let body = String(repeating: "后", count: 200_000)
        guard response.request == request, response.review.files.count == 1,
              let file = response.review.files.first, file.path == "review.txt",
              file.before == nil, file.after?.text == body, file.diff?.contains("+" + body) == true,
              !response.review.conflicted else { throw MobileError.invalidResponse }
        let cards = try await client.snapshot(machine, workspaceID: project.workspaceID)
        guard cards.projects.first?.cards?.isEmpty == true else { throw MobileError.invalidResponse }
        let deliveries = try await client.taskDeliveries(machine, scope: request)
        guard deliveries.items.count == 1, let delivery = deliveries.items.first, delivery.available,
              deliveries.nodeID == nil, delivery.acceptance == nil else { throw MobileError.invalidResponse }
        print("NATIVE_PROJECT_HISTORY_WITHOUT_CARD_OK")
        let frozen = try await client.projectReview(machine, request: ProjectReviewRequest(workspaceID: project.workspaceID, routeID: route, deliveryKey: delivery.outcome.key))
        let original = String(repeating: "变", count: 200_000)
        guard frozen.delivery?.fingerprint == delivery.fingerprint,
              frozen.review.files.first?.after?.text == original,
              frozen.review.files.first?.diff?.contains("+" + original) == true,
              frozen.review.files.first?.after?.text != file.after?.text else { throw MobileError.invalidResponse }
        print("NATIVE_FROZEN_DELIVERY_BYTES_OK")
        guard let reference = frozen.delivery else { throw MobileError.invalidResponse }
        let pending = try await client.prepareDeliveryAcceptance(machine, workspaceID: project.workspaceID, routeID: route,
                                                                revision: cards.grant.revision, delivery: reference)
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = PendingCommandStore(url: directory.appendingPathComponent("commands.json"))
        try await store.save(pending)
        _ = try await client.submit(pending, to: machine)
        guard let originalRequest = try await store.load().first else { throw MobileError.invalidResponse }
        var receipt = try await client.receipt(for: originalRequest, on: machine)
        //20×250ms=5s，覆盖本地SQLite保存与WSS调度；只查询，不重放。
        for _ in 0..<20 where receipt.state == "accepted" {
            try await Task.sleep(for: .milliseconds(250))
            receipt = try await client.receipt(for: originalRequest, on: machine)
        }
        guard receipt.state == "succeeded", receipt.result?.fingerprint == delivery.fingerprint,
              receipt.result?.turnID == delivery.outcome.key.turnID else { throw MobileError.invalidResponse }
        let accepted = try await client.projectReview(machine, request: frozen.request)
        guard accepted.delivery?.acceptance?.operationID == pending.request.operationID,
              accepted.delivery?.acceptance?.key == delivery.outcome.key,
              accepted.review.files.first?.after?.text == original else { throw MobileError.invalidResponse }
        print("NATIVE_ACCEPTANCE_RECEIPT_OK")
        let refreshed = try await client.taskDeliveries(machine, scope: request)
        guard refreshed.items.count == 1, let history = refreshed.items.first,
              history.acceptance?.operationID == pending.request.operationID,
              history.acceptance?.key == delivery.outcome.key,
              history.acceptance?.fingerprint == delivery.fingerprint,
              history.acceptance?.acceptedAtUnix == accepted.delivery?.acceptance?.acceptedAtUnix else { throw MobileError.invalidResponse }
        print("NATIVE_ACCEPTANCE_HISTORY_OK")
        do {
            _ = try await client.projectReview(machine, request: ProjectReviewRequest(workspaceID: project.workspaceID, routeID: "old-route"))
            throw MobileError.invalidResponse
        } catch MobileError.sessionChanged { }
        print("NATIVE_REAL_PROJECT_REVIEW_OK")
    }
}
