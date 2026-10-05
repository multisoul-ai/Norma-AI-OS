import Foundation

public struct WorkspaceAgentCreateRequest: Codable, Equatable, Sendable {
    public let workspaceID: String
    public let areaID: String
    public let operationID: String
    public let revision: UInt64
    public let profile: String
    public init(workspaceID: String, areaID: String, profile: String, revision: UInt64) {
        self.workspaceID = workspaceID; self.areaID = areaID; self.profile = profile; self.revision = revision
        operationID = UUID().uuidString
    }
    enum CodingKeys: String, CodingKey { case workspaceID = "desktop_workspace_id", areaID = "area_id", operationID = "operation_id", revision = "expected_revision", profile }
    func validate() throws {
        guard [workspaceID, areaID, operationID].allSatisfy(validID), revision > 0, MobileAgentProfile.supports(profile) else { throw AgentCreationError.invalid }
    }
}
public struct CreatedWorkspaceAgent: Codable, Equatable, Sendable {
    public let areaID: String
    public let definitionID: Int64
    public let name: String
    public let profile: String?
    public let card: CardSnapshot?
    enum CodingKeys: String, CodingKey { case areaID = "area_id", definitionID = "definition_id", name, profile, card }
}
public struct AgentCreationReceipt: Codable, Equatable, Sendable {
    public let deviceID: String
    public let macID: String
    public let workspaceID: String
    public let operationID: String
    public let state: String
    public let result: CreatedWorkspaceAgent?
    public let error: String?
    public var completed: Bool { ["succeeded", "failed"].contains(state) }
    enum CodingKeys: String, CodingKey { case deviceID = "device_id", macID = "mac_id", workspaceID = "desktop_workspace_id", operationID = "operation_id", state, result, error }
    func validate(_ request: WorkspaceAgentCreateRequest, macID: String? = nil) throws {
        guard validID(self.macID), validID(deviceID), macID.map({ $0 == self.macID }) ?? true,
              workspaceID == request.workspaceID, operationID == request.operationID,
              ["received", "processing", "succeeded", "failed", "outcome_unknown"].contains(state) else { throw MobileError.invalidResponse }
        if let result {
            guard result.areaID == request.areaID, result.definitionID > 0, !result.name.isEmpty,
                  result.profile.map({ $0 == request.profile }) ?? true else { throw MobileError.invalidResponse }
            try result.card?.validate()
        }
        if state == "succeeded" { guard result?.card != nil else { throw MobileError.invalidResponse } }
    }
}
public struct PendingAgentCreation: Codable, Equatable, Sendable {
    public let macID: String
    public let request: WorkspaceAgentCreateRequest
    public var receipt: AgentCreationReceipt?
    public var rejection: String?
    public init(macID: String, request: WorkspaceAgentCreateRequest, receipt: AgentCreationReceipt? = nil) {
        self.macID = macID; self.request = request; self.receipt = receipt
    }
}
public enum AgentCreationError: LocalizedError, Equatable {
    case invalid, offline, unsupported, noProject, changed, busy, unresolved, failed(String)
    public var errorDescription: String? {
        switch self {
        case .invalid: "创建参数无效，请重新选择。"
        case .offline: "暂时连接不上 Mac。保留了创建参数，请连接后重试。"
        case .unsupported: "这个 Mac 版本尚不支持手机新建智能体，请更新桌面版。"
        case .noProject: "先为工作空间添加已有项目，再创建智能体。"
        case .changed: "工作空间或项目已变化，请刷新后重新选择。"
        case .busy: "正在创建智能体，请稍候。"
        case .unresolved: "上一次创建结果尚未确认，请先查询创建结果；不会重复创建。"
        case .failed("configuration_conflict"), .failed("configuration_changed"): "配置已在 Mac 上变化，请刷新后重试。"
        case .failed("unauthorized"): "这部 iPhone 的控制授权已撤销，请在 Mac 检查。"
        case .failed("project_unavailable"), .failed("project_missing"): "项目目录暂时不可用，请在 Mac 检查。"
        case .failed("agent_start_failed"), .failed("create_failed"): "Agent 启动失败，请在 Mac 检查对应工具的安装与登录状态。"
        case .failed(let code): "Mac 未能完成创建（\(code)）。请检查后重试。"
        }
    }
}

/// Shared by native UI and App Intents. Never consults the current UI selection or sends a task.
public actor AgentCreationService {
    private let store: WorkspaceChatStore
    private var inFlight: Set<String> = []
    public init(store: WorkspaceChatStore) { self.store = store }
    public func create(macID: String, workspaceID: String, areaID: String, profile: String, client: WorkspaceChatClient) async throws -> AgentCreationReceipt {
        let scope = macID + ":" + workspaceID + ":" + areaID
        guard inFlight.insert(scope).inserted else { throw AgentCreationError.busy }
        defer { inFlight.remove(scope) }
        if var pending = try await store.pendingCreation(macID: macID, workspaceID: workspaceID, areaID: areaID) {
            // An explicit retry queries first, then only wakes a received operation with its original ID.
            pending.receipt = try await client.createAgent(pending.request)
            try await store.saveCreation(pending)
            return try await resolve(pending, client: client)
        }
        let catalog: WorkspaceCatalog
        do { catalog = try await client.catalog() }
        catch { if error as? MobileError == .connectionFailed || error as? MobileError == .timedOut { throw AgentCreationError.offline }; throw error }
        guard catalog.macID == macID else { throw MobileError.invalidResponse }
        try await store.saveCatalog(catalog)
        guard catalog.capabilities?.contains("workspace_agent_create_v1") == true else { throw AgentCreationError.unsupported }
        guard let workspace = catalog.workspaces.first(where: { $0.id == workspaceID }), workspace.projects.contains(where: { $0.areaID == areaID }) else { throw AgentCreationError.changed }
        let request = WorkspaceAgentCreateRequest(workspaceID: workspaceID, areaID: areaID, profile: profile, revision: workspace.revision)
        try request.validate()
        var pending = PendingAgentCreation(macID: macID, request: request)
        try await store.saveCreation(pending)
        do {
            let receipt = try await client.createAgent(request)
            try receipt.validate(request, macID: macID)
            pending.receipt = receipt; try await store.saveCreation(pending)
        } catch {
            let mobile = error as? MobileError
            let rejected = mobile == .unauthorized || mobile == .invalidCredential || mobile == .unsupportedVersion || mobile == .wrongComputer || error as? AgentCreationError == .unsupported || error as? WorkspaceChatError == .conflict
            if rejected {
                pending.rejection = error.localizedDescription
                try await store.saveCreation(pending)
                throw error
            }
            throw AgentCreationError.unresolved
        }
        return try await resolve(pending, client: client)
    }
    public func resolve(_ entry: PendingAgentCreation, client: WorkspaceChatClient) async throws -> AgentCreationReceipt {
        var entry = entry
        // 20 × 500ms = 10s observation, below the 30s transport request budget.
        for _ in 0..<20 {
            if let receipt = entry.receipt, receipt.completed {
                if receipt.state == "failed" { throw AgentCreationError.failed(receipt.error ?? "create_failed") }
                return receipt
            }
            if let receipt = try await client.creationReceipt(entry.request) {
                try receipt.validate(entry.request, macID: entry.macID)
                entry.receipt = receipt; try await store.saveCreation(entry)
                if receipt.state == "outcome_unknown" { throw AgentCreationError.unresolved }
                if receipt.completed { continue }
            } else { throw AgentCreationError.unresolved }
            try await Task.sleep(for: .milliseconds(500))
        }
        throw AgentCreationError.unresolved
    }
}
