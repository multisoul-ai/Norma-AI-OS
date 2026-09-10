import Foundation

public struct ApprovalProject: Decodable, Sendable {
    public let project: ProjectSnapshot
    public let items: [ApprovalSnapshot]
    public let error: String?
}

public struct ApprovalFeedSnapshot: Decodable, Sendable {
    public let grant: DeviceGrant
    public let projects: [ApprovalProject]

    func validate(deviceID: String) throws {
        _ = try MachineSnapshot(grant: grant, projects: projects.map(\.project), deviceID: deviceID, workspaceID: nil)
        for source in projects {
            guard source.items.count <= 128, source.error.map(validID) ?? true,
                  (source.project.state == "open" && source.error == nil) || source.items.isEmpty,
                  Set(source.items.map { $0.target.runID + ":" + $0.target.requestID }).count == source.items.count else {
                throw MobileError.invalidResponse
            }
            try source.items.forEach { try $0.validate() }
        }
    }
}

struct ApprovalFeedEvent: Decodable, Sendable {
    let streamID: String
    let sequence: UInt64
    let snapshot: ApprovalFeedSnapshot?
    enum CodingKeys: String, CodingKey { case streamID = "stream_id", sequence, snapshot }
}

struct ApprovalFeedTimeline: Sendable {
    let deviceID: String
    private var streamID: String?
    private var sequence: UInt64 = 0
    private(set) var snapshot: ApprovalFeedSnapshot?
    init(deviceID: String) { self.deviceID = deviceID }

    mutating func apply(_ event: ApprovalFeedEvent) throws -> ApprovalFeedSnapshot {
        do {
            guard validID(event.streamID) else { throw MobileError.eventGap }
            if let streamID {
                guard event.streamID == streamID else { throw MobileError.eventGap }
                if event.snapshot == nil {
                    guard event.sequence == sequence, let snapshot else { throw MobileError.eventGap }
                    return snapshot
                }
                guard sequence < UInt64.max, event.sequence == sequence + 1 else { throw MobileError.eventGap }
            } else { guard event.sequence == 0 else { throw MobileError.eventGap } }
            guard let next = event.snapshot else { throw MobileError.eventGap }
            try next.validate(deviceID: deviceID)
            if let snapshot, snapshot.grant.revision != next.grant.revision { throw MobileError.grantChanged }
            streamID = event.streamID; sequence = event.sequence; snapshot = next
            return next
        } catch { streamID = nil; sequence = 0; snapshot = nil; throw error }
    }
}

public enum ApprovalWatchUpdate: Sendable {
    case current(ApprovalFeedSnapshot)
    /// Old tool input must be cleared until a fresh authenticated baseline arrives.
    case recovering(String)
}

extension MobileClient {
    public func approvalUpdates(_ machine: PairedMachine) -> AsyncThrowingStream<ApprovalWatchUpdate, any Error> {
        AsyncThrowingStream(bufferingPolicy: .bufferingNewest(1)) { continuation in
            let work = Task {
                do {
                    try machine.validate()
                    var attempts = 0
                    while !Task.isCancelled {
                        do {
                            guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
                            let connection = try await PinnedConnection.open(machine, streaming: true)
                            defer { connection.close() }
                            let id = UUID().uuidString
                            let body = try JSONSerialization.data(withJSONObject: ["version":1,"request_id":id,"machine_id":machine.id,
                                "action":["type":"watch_approvals","device_id":credential.deviceID,"token":credential.token]])
                            var data = try await connection.exchange(body)
                            var timeline = ApprovalFeedTimeline(deviceID: credential.deviceID)
                            while !Task.isCancelled {
                                let reply = try WireReply.decode(data, expectedID: id)
                                guard reply.result?.status == "approval_event", let event = reply.result?.approvalEvent else { throw MobileError.invalidResponse }
                                try emitApproval(.current(try timeline.apply(event)), to: continuation)
                                data = try await connection.receive()
                            }
                        } catch {
                            try Task.checkCancellation()
                            let retryable: [MobileError] = [.eventGap, .connectionFailed, .timedOut, .unavailable, .grantChanged]
                            guard attempts < 3, (error as? MobileError).map(retryable.contains) ?? (error is URLError) else { throw error }
                            try emitApproval(.recovering(error.localizedDescription), to: continuation)
                            // Same bounded1+2+4s read-only reconnect policy as project streams; no decision replay.
                            try await Task.sleep(for: .seconds(1 << attempts)); attempts += 1
                        }
                    }
                    continuation.finish()
                } catch { continuation.finish(throwing: error) }
            }
            continuation.onTermination = { _ in work.cancel() }
        }
    }

    private func emitApproval(_ update: ApprovalWatchUpdate, to continuation: AsyncThrowingStream<ApprovalWatchUpdate, any Error>.Continuation) throws {
        switch continuation.yield(update) {
        case .enqueued, .dropped:
            // Every wire event was validated first. Only complete current projections are coalesced;
            // recovery replaces buffered old content, so a busy UI never queues old tool input.
            return
        case .terminated: throw CancellationError()
        @unknown default: throw MobileError.eventGap
        }
    }
}
