import Foundation

public struct CardSnapshot: Codable, Sendable, Identifiable, Equatable {
    public let nodeID: UInt64
    public let profile: String
    public let title: String
    public let mobileControl: Bool
    public let state: String
    public let runID: String?
    public let sessionID: String?
    public let taskTitle: String?
    public let turnID: String?
    public var canInterrupt: Bool? = nil
    public var taskTiming: TaskTiming? = nil
    public var taskProgress: TaskProgress? = nil
    public var taskSummary: String? = nil
    public var isBusy: Bool { ["thinking", "running_tool", "waiting_approval", "interrupting"].contains(state) }
    public var id: UInt64 { nodeID }
    enum CodingKeys: String, CodingKey {
        case nodeID = "node_id", profile, title, mobileControl = "mobile_control", state
        case runID = "run_id", sessionID = "session_id", taskTitle = "task_title", turnID = "turn_id"
        case canInterrupt = "can_interrupt", taskTiming = "task_timing"
        case taskProgress = "task_progress"
        case taskSummary = "task_summary"
    }
    func validate() throws {
        if let summary = taskSummary {
            guard !summary.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  summary.unicodeScalars.count <= 240,
                  summary.unicodeScalars.allSatisfy({ $0.properties.generalCategory != .control }) else { throw MobileError.invalidResponse }
        }
        if let progress = taskProgress {
            guard (1...64).contains(progress.total), (0...progress.total).contains(progress.completed),
                  progress.currentStep.map({ !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && $0.count <= 256 }) ?? true else { throw MobileError.invalidResponse }
        }
        guard (taskTiming?.startedAtUnixMs ?? 1) > 0, nodeID > 0, ["Codex", "Claude", "Cursor", "Grok", "OpenCode"].contains(profile),
              ["idle", "preparing", "ready", "thinking", "running_tool", "waiting_approval", "interrupting", "collecting", "done", "failed", "ended", "unknown"].contains(state),
              title.count <= 256, (taskTitle?.count ?? 0) <= 1024,
              runID.map(validID) ?? true, turnID.map(validID) ?? true, (sessionID?.utf8.count ?? 0) <= 128 else { throw MobileError.invalidResponse }
    }
}

public struct TaskProgress: Codable, Sendable, Equatable {
    public let completed: Int
    public let total: Int
    public let currentStep: String?
    enum CodingKeys: String, CodingKey { case completed, total, currentStep = "current_step" }
}

public struct ProjectSnapshot: Decodable, Sendable, Identifiable, Equatable {
    public let workspaceID: String
    public let displayName: String
    public let state: String
    public let routeID: String?
    public let cardCount: Int?
    /// nil means cards were not requested or the project's live window is unavailable.
    public let cards: [CardSnapshot]?
    public var id: String { workspaceID }
    enum CodingKeys: String, CodingKey {
        case workspaceID = "workspace_id", displayName = "display_name", state
        case routeID = "route_id", cardCount = "card_count", cards
    }
    func validate(includeCards: Bool) throws {
        guard validID(workspaceID), !displayName.isEmpty, displayName.count <= 256 else { throw MobileError.invalidResponse }
        if state == "open" {
            guard let routeID, validID(routeID), let cardCount, cardCount >= 0 else { throw MobileError.invalidResponse }
            if includeCards {
                guard let cards, cards.count == cardCount, Set(cards.map(\.id)).count == cards.count else {
                    throw MobileError.invalidResponse
                }
                try cards.forEach { try $0.validate() }
            } else if cards != nil { throw MobileError.invalidResponse }
        } else {
            guard ["closed", "unavailable"].contains(state), routeID == nil, cardCount == nil, cards == nil else {
                throw MobileError.invalidResponse
            }
        }
    }
}

public struct MachineSnapshot: Sendable {
    public let grant: DeviceGrant
    public let projects: [ProjectSnapshot]

    init(reply: WireReply, deviceID: String, workspaceID: String?) throws {
        guard reply.result?.status == "snapshot", let grant = reply.result?.grant,
              let projects = reply.result?.projects else { throw MobileError.invalidResponse }
        try self.init(grant: grant, projects: projects, deviceID: deviceID, workspaceID: workspaceID)
    }
    init(grant: DeviceGrant, projects: [ProjectSnapshot], deviceID: String, workspaceID: String?) throws {
        try grant.validate(deviceID: deviceID)
        let allowed = Set(grant.projects.map(\.workspaceID))
        if let workspaceID, !allowed.contains(workspaceID) { throw MobileError.unauthorized }
        let expected = workspaceID.map { Set([$0]) } ?? allowed
        guard projects.count == expected.count, Set(projects.map(\.id)) == expected else {
            throw MobileError.invalidResponse
        }
        try projects.forEach { try $0.validate(includeCards: workspaceID != nil) }
        self.grant = grant; self.projects = projects
    }
}

/// Optional for older Macs and restored sessions without an observed start event.
public struct TaskTiming: Codable, Sendable, Equatable {
    public let startedAtUnixMs: UInt64
    public let completedElapsedMs: UInt64?
    enum CodingKeys: String, CodingKey {
        case startedAtUnixMs = "started_at_unix_ms", completedElapsedMs = "completed_elapsed_ms"
    }
    public func elapsedMinutes(at date: Date) -> UInt64 {
        if let completedElapsedMs { return completedElapsedMs / (60 * 1000) }
        // Running elapsed is a display estimate across the phone/Mac wall clocks; completed time uses Mac's monotonic clock.
        return UInt64(max(0, date.timeIntervalSince1970 - Double(startedAtUnixMs) / 1000) / 60)
    }
}
