import Foundation

struct DeliveryFeedEvent: Decodable, Sendable {
    let streamID: String
    let sequence: UInt64
    let snapshot: Snapshot?
    struct Snapshot: Decodable, Sendable {
        let grant: DeviceGrant
        let deliveries: TaskDeliveryPage
    }
    enum CodingKeys: String, CodingKey { case streamID = "stream_id", sequence, snapshot }
}

struct DeliveryFeedTimeline: Sendable {
    let deviceID: String
    let scope: ProjectReviewRequest
    let nodeID: UInt64?
    let before: UInt64?
    let pendingOnly: Bool
    private var streamID: String?
    private var sequence: UInt64 = 0
    private var revision: UInt64?
    private(set) var page: TaskDeliveryPage?
    init(deviceID: String, scope: ProjectReviewRequest, nodeID: UInt64?, before: UInt64?, pendingOnly: Bool) {
        self.deviceID = deviceID; self.scope = scope; self.nodeID = nodeID; self.before = before; self.pendingOnly = pendingOnly
    }
    mutating func apply(_ event: DeliveryFeedEvent) throws -> TaskDeliveryPage? {
        do {
            guard validID(event.streamID) else { throw MobileError.eventGap }
            if let streamID {
                guard streamID == event.streamID else { throw MobileError.eventGap }
                if event.snapshot == nil {
                    guard event.sequence == sequence else { throw MobileError.eventGap }
                    return nil // Keep the last unconsumed page; heartbeats carry no list or count.
                }
                guard sequence < UInt64.max, event.sequence == sequence + 1 else { throw MobileError.eventGap }
            } else { guard event.sequence == 0 else { throw MobileError.eventGap } }
            guard let next = event.snapshot else { throw MobileError.eventGap }
            try next.grant.validate(deviceID: deviceID)
            guard next.grant.projects.contains(where: { $0.id == scope.workspaceID }) else { throw MobileError.unauthorized }
            if let revision, revision != next.grant.revision { throw MobileError.grantChanged }
            let fresh = next.deliveries
            guard fresh.pendingOnly != nil, fresh.pendingCount != nil, fresh.collecting != nil else { throw MobileError.invalidResponse }
            try fresh.validate(for: scope, nodeID: nodeID, before: before, pendingOnly: pendingOnly)
            streamID = event.streamID; sequence = event.sequence; revision = next.grant.revision; page = fresh
            return fresh
        } catch { streamID = nil; sequence = 0; revision = nil; page = nil; throw error }
    }
}

public enum DeliveryWatchUpdate: Sendable {
    case current(TaskDeliveryPage)
    case legacy(TaskDeliveryPage)
    case recovering(String)
}

extension MobileClient {
    public func deliveryUpdates(_ machine: PairedMachine, scope: ProjectReviewRequest, nodeID: UInt64? = nil,
                                before: UInt64? = nil, pendingOnly: Bool = false) -> AsyncThrowingStream<DeliveryWatchUpdate, any Error> {
        AsyncThrowingStream(bufferingPolicy: .bufferingNewest(1)) { continuation in
            let work = Task {
                do {
                    try machine.validate(); try scope.validate()
                    guard scope.deliveryKey == nil, nodeID.map({ $0 > 0 && $0 <= Int64.max }) ?? true,
                          before.map({ $0 > 0 && $0 <= Int64.max }) ?? true else { throw MobileError.invalidResponse }
                    var attempts = 0
                    while !Task.isCancelled {
                        do {
                            guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
                            let connection = try await PinnedConnection.open(machine, streaming: true)
                            defer { connection.close() }
                            let id = UUID().uuidString
                            var action: [String: Any] = ["type":"task_deliveries", "watch":true, "pending_only":pendingOnly,
                                "device_id":credential.deviceID, "token":credential.token,
                                "request":try JSONSerialization.jsonObject(with: JSONEncoder().encode(scope))]
                            if let nodeID { action["node_id"] = nodeID }
                            if let before { action["before"] = before }
                            let body = try JSONSerialization.data(withJSONObject: ["version":1,"request_id":id,"machine_id":machine.id,"action":action])
                            var data = try await connection.exchange(body)
                            if WireReply.rejectsNewRead(data) {
                                connection.close()
                                let page = try await taskDeliveries(machine, scope: scope, nodeID: nodeID, before: before)
                                continuation.yield(.legacy(page)); continuation.finish(); return
                            }
                            var timeline = DeliveryFeedTimeline(deviceID: credential.deviceID, scope: scope, nodeID: nodeID, before: before, pendingOnly: pendingOnly)
                            while !Task.isCancelled {
                                let reply = try WireReply.decode(data, expectedID: id)
                                guard reply.result?.status == "delivery_event", let event = reply.result?.deliveryEvent else { throw MobileError.invalidResponse }
                                if let page = try timeline.apply(event) { continuation.yield(.current(page)) }
                                data = try await connection.receive()
                            }
                        } catch {
                            try Task.checkCancellation()
                            let retryable: [MobileError] = [.eventGap, .connectionFailed, .timedOut, .unavailable, .grantChanged]
                            guard attempts < 3, (error as? MobileError).map(retryable.contains) ?? (error is URLError) else { throw error }
                            continuation.yield(.recovering(error.localizedDescription))
                            // Existing bounded1+2+4s read recovery; never replay an acceptance or feedback command.
                            try await Task.sleep(for: .seconds(1 << attempts)); attempts += 1
                        }
                    }
                    continuation.finish()
                } catch { continuation.finish(throwing: error) }
            }
            continuation.onTermination = { _ in work.cancel() }
        }
    }
}
