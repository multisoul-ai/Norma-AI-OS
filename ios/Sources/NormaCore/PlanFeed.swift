import Foundation

struct PlanFeedEvent: Decodable, Sendable {
    let workspaceID: String
    let planID: String?
    let streamID: String
    let sequence: UInt64
    let snapshot: Snapshot?
    struct Snapshot: Decodable, Sendable {
        let grant: DeviceGrant
        let progress: ProjectPlanProgress?
        init(from decoder: any Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            grant = try container.decode(DeviceGrant.self, forKey: .grant)
            if try container.decodeNil(forKey: .plan) {
                guard try container.nestedUnkeyedContainer(forKey: .decisions).isAtEnd else { throw MobileError.invalidResponse }
                progress = nil
            } else { progress = try ProjectPlanProgress(from: decoder) }
        }
        enum CodingKeys: String, CodingKey { case grant, plan, decisions }
    }
    enum CodingKeys: String, CodingKey { case workspaceID = "workspace_id", planID = "plan_id", streamID = "stream_id", sequence, snapshot }
}

struct PlanFeedTimeline: Sendable {
    let machineID: String
    let deviceID: String
    let workspaceID: String
    let planID: String?
    private var streamID: String?
    private var sequence: UInt64 = 0
    private var revision: UInt64?
    private(set) var progress: ProjectPlanProgress?
    init(machineID: String, deviceID: String, workspaceID: String, planID: String?) {
        self.machineID = machineID; self.deviceID = deviceID; self.workspaceID = workspaceID; self.planID = planID
    }
    mutating func apply(_ event: PlanFeedEvent) throws -> ProjectPlanProgress? {
        do {
            guard event.workspaceID == workspaceID, event.planID == planID, validID(event.streamID) else { throw MobileError.eventGap }
            if let streamID {
                guard event.streamID == streamID else { throw MobileError.eventGap }
                if event.snapshot == nil {
                    guard event.sequence == sequence else { throw MobileError.eventGap }
                    return nil // A heartbeat must not replace an unconsumed snapshot in bufferingNewest(1).
                }
                guard sequence < UInt64.max, event.sequence == sequence + 1 else { throw MobileError.eventGap }
            } else { guard event.sequence == 0 else { throw MobileError.eventGap } }
            guard let next = event.snapshot else { throw MobileError.eventGap }
            try next.grant.validate(deviceID: deviceID)
            guard next.grant.projects.contains(where: { $0.id == workspaceID }) else { throw MobileError.unauthorized }
            if let revision, revision != next.grant.revision { throw MobileError.grantChanged }
            if let fresh = next.progress {
                try fresh.validate(workspaceID: workspaceID, planID: planID ?? fresh.plan.id, machineID: machineID)
                if let progress, progress.plan != fresh.plan {
                    guard planID == nil, progress.plan.id != fresh.plan.id,
                          fresh.plan.sequence > progress.plan.sequence else { throw MobileError.invalidResponse }
                }
            } else {
                guard planID == nil, progress == nil else { throw MobileError.invalidResponse }
            }
            streamID = event.streamID; sequence = event.sequence; revision = next.grant.revision; progress = next.progress
            return next.progress
        } catch { streamID = nil; sequence = 0; revision = nil; progress = nil; throw error }
    }
}

public enum PlanWatchUpdate: Sendable {
    case current(ProjectPlanProgress)
    case legacy(ProjectPlanProgress?)
    case empty
    case recovering(String)
}

extension MobileClient {
    public func planUpdates(_ machine: PairedMachine, workspaceID: String, planID: String? = nil) -> AsyncThrowingStream<PlanWatchUpdate, any Error> {
        AsyncThrowingStream(bufferingPolicy: .bufferingNewest(1)) { continuation in
            let work = Task {
                do {
                    try machine.validate()
                    guard validID(workspaceID), planID.map(validID) ?? true else { throw MobileError.invalidResponse }
                    var attempts = 0
                    while !Task.isCancelled {
                        do {
                            guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
                            let connection = try await PinnedConnection.open(machine, streaming: true)
                            defer { connection.close() }
                            let id = UUID().uuidString
                            var action: [String: Any] = ["type":"watch_plan","device_id":credential.deviceID,"token":credential.token,"workspace_id":workspaceID]
                            if let planID { action["plan_id"] = planID }
                            let body = try JSONSerialization.data(withJSONObject: ["version":1,"request_id":id,"machine_id":machine.id,"action":action])
                            var data = try await connection.exchange(body)
                            if WireReply.rejectsNewRead(data) {
                                connection.close()
                                var savedID = planID
                                if savedID == nil { savedID = try await projectPlans(machine, workspaceID: workspaceID).items.first?.id }
                                var saved: ProjectPlanProgress?
                                if let savedID { saved = try await projectPlan(machine, workspaceID: workspaceID, planID: savedID) }
                                continuation.yield(.legacy(saved)); continuation.finish(); return
                            }
                            var timeline = PlanFeedTimeline(machineID: machine.id, deviceID: credential.deviceID, workspaceID: workspaceID, planID: planID)
                            while !Task.isCancelled {
                                let reply = try WireReply.decode(data, expectedID: id)
                                guard reply.result?.status == "plan_event", let event = reply.result?.planEvent else { throw MobileError.invalidResponse }
                                let progress = try timeline.apply(event)
                                if event.snapshot != nil {
                                    if let progress { continuation.yield(.current(progress)) }
                                    else { continuation.yield(.empty) }
                                }
                                data = try await connection.receive()
                            }
                        } catch {
                            try Task.checkCancellation()
                            let retryable: [MobileError] = [.eventGap, .connectionFailed, .timedOut, .unavailable, .grantChanged]
                            guard attempts < 3, (error as? MobileError).map(retryable.contains) ?? (error is URLError) else { throw error }
                            continuation.yield(.recovering(error.localizedDescription))
                            // Same bounded1+2+4s read-only recovery as existing watches. Never replay a decision.
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
