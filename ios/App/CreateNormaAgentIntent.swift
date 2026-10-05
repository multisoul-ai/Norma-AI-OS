import AppIntents
import NormaCore

enum NormaAgentKind: String, AppEnum {
    case codex = "Codex", claude = "Claude", cursor = "Cursor"
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Agent 类型"
    static let caseDisplayRepresentations: [Self: DisplayRepresentation] = [
        .codex: "Codex", .claude: "Claude", .cursor: "Cursor"
    ]
}

struct NormaWorkspaceEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "工作空间"
    static let defaultQuery = NormaWorkspaceQuery()
    let macID: String
    let workspaceID: String
    let name: String
    let macName: String
    var id: String { macID + "/" + workspaceID }
    var displayRepresentation: DisplayRepresentation { DisplayRepresentation(title: "\(name)", subtitle: "\(macName)") }
}
struct NormaProjectEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "所属项目"
    static let defaultQuery = NormaProjectQuery()
    let macID: String
    let workspaceID: String
    let areaID: String
    let name: String
    let location: String
    var id: String { macID + "/" + workspaceID + "/" + areaID }
    var displayRepresentation: DisplayRepresentation { DisplayRepresentation(title: "\(name)", subtitle: "\(location)") }
}
struct NormaWorkspaceQuery: EntityQuery {
    func entities(for identifiers: [String]) async throws -> [NormaWorkspaceEntity] {
        try await suggestedEntities().filter { identifiers.contains($0.id) }
    }
    func suggestedEntities() async throws -> [NormaWorkspaceEntity] {
        try await AgentCreationEnvironment.shared.catalogs().flatMap { machine, catalog in
            catalog.workspaces.map { NormaWorkspaceEntity(macID: machine.id, workspaceID: $0.id, name: $0.name, macName: machine.name) }
        }
    }
}
struct NormaProjectQuery: EntityQuery {
    @IntentParameterDependency<CreateNormaAgentIntent>(\.$workspace) var intent
    func entities(for identifiers: [String]) async throws -> [NormaProjectEntity] {
        try await allProjects().filter { identifiers.contains($0.id) }
    }
    func suggestedEntities() async throws -> [NormaProjectEntity] {
        let projects = try await allProjects()
        guard let workspace = intent?.workspace else { return projects }
        return projects.filter { $0.macID == workspace.macID && $0.workspaceID == workspace.workspaceID }
    }
    private func allProjects() async throws -> [NormaProjectEntity] {
        try await AgentCreationEnvironment.shared.catalogs().flatMap { machine, catalog in
            catalog.workspaces.flatMap { workspace in
                workspace.projects.map { NormaProjectEntity(macID: machine.id, workspaceID: workspace.id, areaID: $0.areaID, name: $0.name, location: "\(machine.name) · \(workspace.name) · \($0.root)") }
            }
        }
    }
}

struct CreateNormaAgentIntent: AppIntent {
    static let title: LocalizedStringResource = "创建 Norma 智能体"
    static let description = IntentDescription("在指定电脑的工作空间与项目中创建新的 Agent。创建本身不会发送任务。")
    static let openAppWhenRun = false
    static let authenticationPolicy: IntentAuthenticationPolicy = .requiresLocalDeviceAuthentication
    @Parameter(title: "工作空间") var workspace: NormaWorkspaceEntity
    @Parameter(title: "所属项目") var project: NormaProjectEntity
    @Parameter(title: "Agent 类型", default: .codex) var kind: NormaAgentKind
    static var parameterSummary: some ParameterSummary { Summary("在\(\.$workspace)的\(\.$project)创建\(\.$kind)") }

    func perform() async throws -> some IntentResult & ReturnsValue<String> & ProvidesDialog {
        guard workspace.macID == project.macID, workspace.workspaceID == project.workspaceID else { throw AgentCreationError.changed }
        let receipt = try await AgentCreationEnvironment.shared.create(macID: workspace.macID, workspaceID: workspace.workspaceID, areaID: project.areaID, profile: kind.rawValue)
        guard let agent = receipt.result else { throw AgentCreationError.unresolved }
        let result = "已创建 \(agent.name)，项目：\(project.name)，状态：\(workspaceChatStatus(agent.card?.state ?? "preparing"))。"
        return .result(value: result, dialog: "\(result)")
    }
}

struct NormaAgentShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(intent: CreateNormaAgentIntent(), phrases: ["用\(.applicationName)创建智能体", "创建\(.applicationName)智能体"], shortTitle: "创建智能体", systemImageName: "person.crop.circle.badge.plus")
    }
}
