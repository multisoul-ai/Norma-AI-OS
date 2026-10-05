import Foundation

public enum WorkspaceMessageIntent: String, Codable, Sendable { case discuss, execute }

public struct WorkspaceAgentTarget: Codable, Equatable, Sendable {
    public let areaID: String
    public let definitionID: Int64
    public let nodeID: UInt64
    public let runID: String?
    public let sessionID: String?
    public var taskRef: String?
    public init(areaID: String, definitionID: Int64, nodeID: UInt64, runID: String?, sessionID: String?, taskRef: String? = nil) {
        self.areaID = areaID; self.definitionID = definitionID; self.nodeID = nodeID
        self.runID = runID; self.sessionID = sessionID; self.taskRef = taskRef
    }
    enum CodingKeys: String, CodingKey { case areaID = "area_id", definitionID = "definition_id", nodeID = "node_id", runID = "run_id", sessionID = "session_id", taskRef = "task_ref" }
}

public struct WorkspaceChatMessage: Codable, Equatable, Identifiable, Sendable {
    public let workspaceID: String
    public let operationID: String
    public let messageID: String
    public let text: String
    public let intent: WorkspaceMessageIntent
    public let target: WorkspaceAgentTarget?
    public let ideaID: String?
    public var id: String { messageID }
    public init(workspaceID: String, text: String, intent: WorkspaceMessageIntent = .discuss, target: WorkspaceAgentTarget? = nil, ideaID: String? = nil) {
        self.workspaceID = workspaceID; self.text = text; self.intent = intent; self.target = target; self.ideaID = ideaID
        operationID = UUID().uuidString; messageID = UUID().uuidString
    }
    enum CodingKeys: String, CodingKey { case workspaceID = "desktop_workspace_id", operationID = "operation_id", messageID = "message_id", text, intent, target, ideaID = "idea_id" }
    func validate() throws {
        guard [workspaceID,operationID,messageID].allSatisfy(validID), !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              text.utf8.count <= 64 * 1024, ideaID.map(validID) ?? true else { throw WorkspaceChatError.invalid }
        if let target { guard validID(target.areaID), target.nodeID > 0, target.definitionID > 0 else { throw WorkspaceChatError.invalid } }
    }
}

public struct WorkspaceCatalog: Codable, Equatable, Sendable {
    public let macID: String
    public let workspaces: [DesktopChatWorkspace]
    public let availableProjects: [DesktopAvailableProject]
    public let capabilities: [String]?
    enum CodingKeys: String, CodingKey { case macID = "mac_id", workspaces, availableProjects = "available_projects", capabilities }
}
public struct DesktopChatWorkspace: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let name: String
    public let revision: UInt64
    public let projects: [DesktopChatProject]
    public var agents: [DesktopChatMember] {
        projects.flatMap { project in project.agents.map { DesktopChatMember(project: project, agent: $0) } }
    }
}
public struct DesktopAvailableProject: Codable, Equatable, Identifiable, Sendable {
    public let projectID: String
    public let name: String
    public let root: String
    public var id: String { projectID }
    enum CodingKeys: String, CodingKey { case projectID = "project_id", name, root }
}
public struct DesktopChatProject: Codable, Equatable, Identifiable, Sendable {
    public let areaID: String
    public let projectID: String
    public let name: String
    public let root: String
    public let agents: [DesktopChatAgent]
    public let definitions: [DesktopChatDefinition]
    public var id: String { areaID }
    enum CodingKeys: String, CodingKey { case areaID = "area_id", projectID = "project_id", name, root, agents, definitions }
}
public struct DesktopChatDefinition: Codable, Equatable, Identifiable, Sendable {
    public let id: Int64
    public let name: String
    public let profile: String
    public let enabled: Bool
}
public struct DesktopChatAgent: Codable, Equatable, Identifiable, Sendable {
    public let definitionID: Int64
    public let name: String
    public let profile: String
    public let card: CardSnapshot
    public var id: UInt64 { card.nodeID }
    enum CodingKeys: String, CodingKey { case definitionID = "definition_id", name, profile, card }
}
public struct DesktopChatMember: Equatable, Identifiable, Sendable {
    public let project: DesktopChatProject
    public let agent: DesktopChatAgent
    public var id: String { project.areaID + "/" + String(agent.card.nodeID) }
    public var target: WorkspaceAgentTarget { WorkspaceAgentTarget(areaID: project.areaID, definitionID: agent.definitionID, nodeID: agent.card.nodeID, runID: agent.card.runID, sessionID: agent.card.sessionID) }
}
public struct WorkspaceTaskReference: Codable, Equatable, Sendable {
    public let areaID: String
    public let nodeID: UInt64
    public let taskRef: String
    public let state: String
    enum CodingKeys: String, CodingKey { case areaID = "area_id", nodeID = "node_id", taskRef = "task_ref", state }
}
public struct WorkspaceEventPayload: Codable, Equatable, Sendable {
    public let text: String?
    public let sender: String?
    public let state: String?
    public let error: String?
    public let target: WorkspaceAgentTarget?
    public let taskRef: String?
    public let tasks: [WorkspaceTaskReference]?
    public let messageID: String?
    public let ideaID: String?
    enum CodingKeys: String, CodingKey { case text, sender, state, error, target, tasks, taskRef = "task_ref", messageID = "message_id", ideaID = "idea_id" }
}
public struct WorkspaceChatReceipt: Codable, Equatable, Sendable {
    public let deviceID: String
    public let macID: String
    public let workspaceID: String
    public let operationID: String
    public let state: String
    public let cursor: UInt64
    public let result: WorkspaceEventPayload?
    enum CodingKeys: String, CodingKey { case deviceID = "device_id", macID = "mac_id", workspaceID = "desktop_workspace_id", operationID = "operation_id", state, cursor, result }
}
public struct WorkspaceChatEvent: Codable, Equatable, Identifiable, Sendable {
    public let macID: String
    public let workspaceID: String
    public let cursor: UInt64
    public let deviceID: String
    public let operationID: String
    public let kind: String
    public let createdAtUnixMS: Int64?
    public let payload: WorkspaceEventPayload
    public var id: UInt64 { cursor }
    enum CodingKeys: String, CodingKey { case macID = "mac_id", workspaceID = "desktop_workspace_id", cursor, deviceID = "device_id", operationID = "operation_id", kind, payload, createdAtUnixMS = "created_at_unix_ms" }
}
public struct WorkspaceOutboxEntry: Codable, Equatable, Identifiable, Sendable {
    public let macID: String
    public let message: WorkspaceChatMessage
    public var receipt: WorkspaceChatReceipt?
    public var id: String { message.operationID }
}
public enum WorkspaceChatError: LocalizedError, Equatable {
    case invalid, damagedStore, conflict, unavailable, targetChanged, server(String)
    public var errorDescription: String? {
        switch self {
        case .invalid: "消息或执行目标无效，原文已保留。"
        case .damagedStore: "无法保存群聊记录，请保留当前输入并重试。"
        case .conflict: "工作空间配置已在 Mac 上变化，请刷新后重新修改。"
        case .unavailable: "Mac 暂时无法连接，消息会保留为待发送。"
        case .targetChanged: "指定卡片或会话已变化，请重新选择；原文已保留。"
        case .server("mac_restarted"): "Mac 已重启，原会话可能已结束。请核查原操作回执，不会自动重发。"
        case .server("outcome_unknown"): "执行结果暂时无法确认，请核查原操作回执；不会自动重发。"
        case .server("unauthorized"), .server("grant_changed"): "这部 iPhone 的 Mac 授权已撤销或变化，请在 Mac 检查；原文已保留。"
        case .server("norma_model_unconfigured"): "Mac 的 Norma 模型尚未配置，请在 Mac 设置中选择模型；群聊原文已保留。"
        case .server("norma_unavailable"): "Mac 的 Norma 暂时无法回应，请检查 Mac 模型连接；原文已保留。"
        case .server("target_missing"), .server("target_ambiguous"), .server("session_changed"), .server("route_expired"): "指定卡片或原会话已变化，请重新选择；不会自动替换执行目标。"
        case .server("configuration_changed"): "工作空间配置已变化，请刷新后检查项目与成员；原文已保留。"
        case .server("busy"): "指定卡片当前无法接收消息，请查看 Mac 的输入或确认状态；原文已保留。"
        case .server("project_unavailable"): "关联项目无法加载，请在 Mac 检查项目路径；原文已保留。"
        case .server("desktop_host_unavailable"): "Mac 工作空间尚未就绪，请打开桌面工作空间后重试。"
        case .server("workspace_missing"): "这个工作空间已从 Mac 移除；历史和原文已保留。"
        case .server("project_missing"): "关联项目已变化，请刷新工作空间设置；原文已保留。"
        case .server("agent_missing"), .server("agent_disabled"): "指定 Agent 已移除或停用，请重新选择；原文已保留。"
        case .server: "Mac 未能完成这条消息，请查看 Mac 的执行记录；原文已保留。"
        }
    }
}

public struct WorkspaceChatDraft: Codable, Equatable, Sendable {
    public var text: String
    public var intent: WorkspaceMessageIntent
    public var target: WorkspaceAgentTarget?
    public var ideaID: String?
    public init(text: String = "", intent: WorkspaceMessageIntent = .discuss, target: WorkspaceAgentTarget? = nil, ideaID: String? = nil) {
        self.text = text; self.intent = intent; self.target = target; self.ideaID = ideaID
    }
}
