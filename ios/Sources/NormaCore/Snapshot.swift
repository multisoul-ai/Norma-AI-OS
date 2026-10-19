import Foundation

public enum MobileAgentProfile {
    public static let supported = ["Codex", "Claude", "Cursor"]
    /// Mac 端可能出现的全部 Agent 卡片 profile（与 Rust 侧 `canvas::AgentProfile` 的名称一致）。
    /// 只用于校验和展示：不在这里的 profile 会让整份快照解码失败；能否在手机端创建/操作仍由 `supported` 决定。
    public static let displayable = ["Codex", "Claude", "Cursor", "Grok", "OpenCode", "InfCode", "Kimi", "ZCode", "Pi", "Hermes"]
    public static func supports(_ profile: String) -> Bool { supported.contains(profile) }
    public static func historyPaginatesBackward(_ profile: String) -> Bool { profile == "Codex" }
}

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
    public var supportsInterruptAction: Bool { canInterrupt != false }
    public var showsInterruptAction: Bool { supportsInterruptAction && isBusy }
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
        guard (taskTiming?.startedAtUnixMs ?? 1) > 0, nodeID > 0, MobileAgentProfile.displayable.contains(profile),
              ["idle", "preparing", "ready", "thinking", "running_tool", "waiting_approval", "interrupting", "collecting", "done", "failed", "ended", "unknown"].contains(state),
              title.count <= 256, (taskTitle?.count ?? 0) <= 1024,
              runID.map(validID) ?? true, turnID.map(validID) ?? true, (sessionID?.utf8.count ?? 0) <= 128 else { throw MobileError.invalidResponse }
    }
}

public struct ProjectAgentDefinition: Decodable, Sendable, Identifiable, Equatable {
    public let definitionID: Int64
    public let name: String
    public let speechAlias: String?
    public let profile: String
    public let enabled: Bool
    public let activeNodeID: UInt64?
    public let busy: Bool
    public var id: Int64 { definitionID }
    public var isCreatableOnIOS: Bool {
        enabled && activeNodeID == nil && MobileAgentProfile.supports(profile)
    }
    enum CodingKeys: String, CodingKey {
        case definitionID = "definition_id", name, speechAlias = "speech_alias", profile, enabled
        case activeNodeID = "active_node_id", busy
    }
    func validate() throws {
        let validText = { (value: String, limit: Int) in
            !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                && value.unicodeScalars.count <= limit
                && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
        }
        guard definitionID > 0, validText(name, 256),
              MobileAgentProfile.displayable.contains(profile),
              activeNodeID.map({ $0 > 0 }) ?? true,
              speechAlias.map({ validText($0, 256) }) ?? true else { throw MobileError.invalidResponse }
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
    public let agentDefinitions: [ProjectAgentDefinition]
    public var creatableAgentDefinitions: [ProjectAgentDefinition] {
        agentDefinitions.filter(\.isCreatableOnIOS)
    }

    init(reply: WireReply, deviceID: String, workspaceID: String?) throws {
        guard let result = reply.result, result.status == "snapshot", let grant = result.grant,
              let projects = result.projects else { throw MobileError.invalidResponse }
        try self.init(grant: grant, projects: projects, agentDefinitions: result.agents ?? [], deviceID: deviceID, workspaceID: workspaceID)
    }
    init(grant: DeviceGrant, projects: [ProjectSnapshot], agentDefinitions: [ProjectAgentDefinition] = [], deviceID: String, workspaceID: String?) throws {
        try grant.validate(deviceID: deviceID)
        let allowed = Set(grant.projects.map(\.workspaceID))
        if let workspaceID, !allowed.contains(workspaceID) { throw MobileError.unauthorized }
        let expected = workspaceID.map { Set([$0]) } ?? allowed
        guard projects.count == expected.count, Set(projects.map(\.id)) == expected else {
            throw MobileError.invalidResponse
        }
        try projects.forEach { try $0.validate(includeCards: workspaceID != nil) }
        guard workspaceID != nil || agentDefinitions.isEmpty,
              agentDefinitions.count <= 256,
              Set(agentDefinitions.map(\.id)).count == agentDefinitions.count,
              Set(agentDefinitions.map { $0.name.lowercased() }).count == agentDefinitions.count else {
            throw MobileError.invalidResponse
        }
        try agentDefinitions.forEach { try $0.validate() }
        self.grant = grant; self.projects = projects; self.agentDefinitions = agentDefinitions
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
