import Foundation

struct ProjectEvent: Decodable, Sendable, Equatable {
    let streamID: String
    let workspaceID: String
    let sequence: UInt64
    let kind: String
    let project: ProjectSnapshot?
    let displayName: String?
    let upserted: [CardSnapshot]?
    let removed: [UInt64]?
    let order: [UInt64]?
    enum CodingKeys: String, CodingKey {
        case streamID = "stream_id", workspaceID = "workspace_id", sequence, kind, project, displayName = "display_name", upserted, removed, order
    }
}

struct ProjectTimeline: Sendable {
    let workspaceID: String
    private(set) var project: ProjectSnapshot?
    private var last: ProjectEvent?
    init(workspaceID: String) { self.workspaceID = workspaceID }

    mutating func apply(_ event: ProjectEvent) throws -> ProjectSnapshot {
        do { return try advance(event) }
        catch { project = nil; last = nil; throw error }
    }
    private mutating func advance(_ event: ProjectEvent) throws -> ProjectSnapshot {
        guard validID(event.streamID), event.workspaceID == workspaceID else { throw MobileError.eventGap }
        if let last {
            guard event.streamID == last.streamID else { throw MobileError.eventGap }
            if event == last, let project { return project }
            if event.kind == "heartbeat" {
                guard event.sequence == last.sequence, event.project == nil, event.upserted == nil,
                      event.removed == nil, event.order == nil, event.displayName == nil, let project else { throw MobileError.eventGap }
                return project
            }
            guard last.sequence < UInt64.max, event.sequence == last.sequence + 1 else { throw MobileError.eventGap }
        } else if event.kind != "snapshot" || event.sequence != 0 { throw MobileError.eventGap }

        let next: ProjectSnapshot
        switch event.kind {
        case "snapshot":
            guard let snapshot = event.project, event.upserted == nil, event.removed == nil,
                  event.order == nil, event.displayName == nil, snapshot.id == workspaceID else { throw MobileError.eventGap }
            next = snapshot
        case "delta":
            guard let project, project.state == "open", let cards = project.cards,
                  let name = event.displayName, let upserts = event.upserted, let removed = event.removed,
                  let order = event.order, event.project == nil,
                  Set(upserts.map(\.id)).count == upserts.count, Set(removed).count == removed.count,
                  Set(order).count == order.count, Set(upserts.map(\.id)).isDisjoint(with: removed) else { throw MobileError.eventGap }
            var byID = Dictionary(uniqueKeysWithValues: cards.map { ($0.id, $0) })
            for id in removed {
                guard byID.removeValue(forKey: id) != nil else { throw MobileError.eventGap }
            }
            for card in upserts { try card.validate(); byID[card.id] = card }
            guard Set(order) == Set(byID.keys) else { throw MobileError.eventGap }
            next = ProjectSnapshot(workspaceID: workspaceID, displayName: name, state: "open", routeID: project.routeID,
                                   cardCount: order.count, cards: order.compactMap { byID[$0] })
        default: throw MobileError.eventGap
        }
        try next.validate(includeCards: true)
        project = next; last = event
        return next
    }
}

public enum ProjectWatchUpdate: Sendable {
    case current(MachineSnapshot)
    /// A reconnect is read-only; the consumer must clear actionable old state until a new baseline.
    case recovering(String)
}

extension MobileClient {
    public func projectUpdates(_ machine: PairedMachine, workspaceID: String) -> AsyncThrowingStream<ProjectWatchUpdate, any Error> {
        AsyncThrowingStream(bufferingPolicy: .bufferingNewest(1)) { continuation in
            let work = Task {
                do {
                    try machine.validate()
                    guard validID(workspaceID) else { throw MobileError.invalidResponse }
                    var attempts = 0
                    while !Task.isCancelled {
                        do {
                            guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
                            let connection = try await PinnedConnection.open(machine, streaming: true)
                            defer { connection.close() }
                            let id = UUID().uuidString
                            let body = try JSONSerialization.data(withJSONObject: ["version": 1, "request_id": id, "machine_id": machine.id,
                                "action": ["type": "watch_project", "device_id": credential.deviceID, "token": credential.token, "workspace_id": workspaceID]])
                            var data = try await connection.exchange(body)
                            var timeline = ProjectTimeline(workspaceID: workspaceID)
                            var revision: UInt64?
                            var agentDefinitions: [ProjectAgentDefinition] = []
                            while !Task.isCancelled {
                                let reply = try WireReply.decode(data, expectedID: id)
                                guard reply.result?.status == "project_event", let event = reply.result?.event,
                                      let grant = reply.result?.grant else { throw MobileError.invalidResponse }
                                if let revision, revision != grant.revision { throw MobileError.grantChanged }
                                let project = try timeline.apply(event)
                                if let agents = reply.result?.agents { agentDefinitions = agents }
                                let snapshot = try MachineSnapshot(grant: grant, projects: [project], agentDefinitions: agentDefinitions, deviceID: credential.deviceID, workspaceID: workspaceID)
                                revision = grant.revision
                                try emit(.current(snapshot), to: continuation)
                                data = try await connection.receive()
                            }
                        } catch {
                            try Task.checkCancellation()
                            let retryable = [MobileError.eventGap, .connectionFailed, .timedOut, .unavailable, .grantChanged]
                            guard attempts < 3, (error as? MobileError).map(retryable.contains) ?? (error is URLError) else { throw error }
                            try emit(.recovering(error.localizedDescription), to: continuation)
                            // Three bounded read-only reconnects after 1 + 2 + 4 = 7s backoff; no command is replayed.
                            try await Task.sleep(for: .seconds(1 << attempts)); attempts += 1
                        }
                    }
                    continuation.finish()
                } catch { continuation.finish(throwing: error) }
            }
            continuation.onTermination = { _ in work.cancel() }
        }
    }
    private func emit(_ update: ProjectWatchUpdate, to continuation: AsyncThrowingStream<ProjectWatchUpdate, any Error>.Continuation) throws {
        switch continuation.yield(update) {
        case .enqueued, .dropped:
            // Every wire delta was already validated and applied by ProjectTimeline. The UI gets
            // complete snapshots, so a busy consumer may coalesce them without losing any cards.
            // Recovery also replaces buffered old state; raw sequence gaps still fail validation.
            return
        case .terminated: throw CancellationError()
        @unknown default: throw MobileError.eventGap
        }
    }
}
