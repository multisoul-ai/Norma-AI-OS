import Foundation

public struct WorkspaceConfigurationChange: Codable, Sendable {
    public let kind: String
    public var name: String?
    public var projectID: String?
    public var areaID: String?
    public var definitionID: Int64?
    public var nodeID: UInt64?
    public init(kind: String, name: String? = nil, projectID: String? = nil, areaID: String? = nil, definitionID: Int64? = nil, nodeID: UInt64? = nil) {
        self.kind = kind; self.name = name; self.projectID = projectID; self.areaID = areaID; self.definitionID = definitionID; self.nodeID = nodeID
    }
    enum CodingKeys: String, CodingKey { case kind, name, projectID = "project_id", areaID = "area_id", definitionID = "definition_id", nodeID = "node_id" }
}
public protocol WorkspaceChatAPI: Sendable {
    func activity(_ request: WorkspaceActivityRequest) async throws -> WorkspaceActivityResponse
    func createAgent(_ request: WorkspaceAgentCreateRequest) async throws -> AgentCreationReceipt
    func creationReceipt(_ request: WorkspaceAgentCreateRequest) async throws -> AgentCreationReceipt?
    func close() async
    func catalog() async throws -> WorkspaceCatalog
    func receipt(for message: WorkspaceChatMessage) async throws -> WorkspaceChatReceipt?
    func submit(_ message: WorkspaceChatMessage) async throws -> WorkspaceChatReceipt
    func events(workspaceID: String, after: UInt64) async throws -> [WorkspaceChatEvent]
    func configure(workspaceID: String, revision: UInt64, change: WorkspaceConfigurationChange) async throws
}

public extension WorkspaceChatAPI {
    func activity(_ request: WorkspaceActivityRequest) async throws -> WorkspaceActivityResponse { throw MobileError.unsupportedVersion }
    func close() async {}
    func createAgent(_ request: WorkspaceAgentCreateRequest) async throws -> AgentCreationReceipt { throw MobileError.unsupportedVersion }
    func creationReceipt(_ request: WorkspaceAgentCreateRequest) async throws -> AgentCreationReceipt? { throw MobileError.unsupportedVersion }
}

/// Direct first. A transport change always queries the same operation before resubmission.
public actor WorkspaceChatClient {
    private let direct: any WorkspaceChatAPI
    private let relayFactory: @Sendable () async throws -> any WorkspaceChatAPI
    private var relay: (any WorkspaceChatAPI)?
    public private(set) var connectionName = "局域网"
    public init(direct: any WorkspaceChatAPI, relay: @escaping @Sendable () async throws -> any WorkspaceChatAPI) {
        self.direct = direct; relayFactory = relay
    }
    private func fallback() async throws -> any WorkspaceChatAPI {
        if let relay { connectionName = "远程中继"; return relay }
        let created = try await relayFactory(); relay = created; connectionName = "远程中继"; return created
    }
    public func activity(_ request: WorkspaceActivityRequest) async throws -> WorkspaceActivityResponse {
        guard validID(request.workspaceID) || (request.workspaceID.isEmpty && ["summary","subscription","set_subscription"].contains(request.kind)) else { throw MobileError.invalidResponse }
        try request.target?.validate();try request.registration?.validate()
        if request.isMutation {
            let api:any WorkspaceChatAPI
            do { _ = try await direct.catalog();api=direct }
            catch {guard canSwitch(error) else {throw error};api=try await fallback()}
            return try await api.activity(request) // Unknown writes are resolved only by their original receipt.
        }
        do {return try await direct.activity(request)}
        catch {guard canSwitch(error) else {throw error};return try await fallback().activity(request)}
    }
    public func catalog() async throws -> WorkspaceCatalog {
        do { let value = try await direct.catalog(); connectionName = "局域网"; return value }
        catch { guard canSwitch(error) else { throw error }; return try await fallback().catalog() }
    }
    public func createAgent(_ request: WorkspaceAgentCreateRequest) async throws -> AgentCreationReceipt {
        try request.validate()
        do { return try await create(request, through: direct) }
        catch {
            guard canSwitch(error) else { throw error }
            do { return try await create(request, through: fallback()) }
            catch { throw AgentCreationError.unresolved } // The first connection may already have created it.
        }
    }
    private func create(_ request: WorkspaceAgentCreateRequest, through api: any WorkspaceChatAPI) async throws -> AgentCreationReceipt {
        if let receipt = try await api.creationReceipt(request), receipt.state != "received" { try receipt.validate(request); return receipt }
        let receipt = try await api.createAgent(request); try receipt.validate(request); return receipt
    }
    public func creationReceipt(_ request: WorkspaceAgentCreateRequest) async throws -> AgentCreationReceipt? {
        do { return try await direct.creationReceipt(request) }
        catch { guard canSwitch(error) else { throw error }; return try await fallback().creationReceipt(request) }
    }
    public func deliver(_ message: WorkspaceChatMessage) async throws -> WorkspaceChatReceipt {
        try message.validate()
        do { let value = try await deliver(message, through: direct); connectionName = "局域网"; return value }
        catch { guard canSwitch(error) else { throw error }; return try await deliver(message, through: fallback()) }
    }
    private func deliver(_ message: WorkspaceChatMessage, through api: any WorkspaceChatAPI) async throws -> WorkspaceChatReceipt {
        if let receipt = try await api.receipt(for: message), receipt.state != "received" { return receipt }
        // Resubmitting `received` wakes a lost worker. The Mac atomic claim prevents double execution.
        return try await api.submit(message)
    }
    public func query(_ message: WorkspaceChatMessage) async throws -> WorkspaceChatReceipt? {
        do { return try await direct.receipt(for: message) }
        catch { guard canSwitch(error) else { throw error }; return try await fallback().receipt(for: message) }
    }
    public func events(workspaceID: String, after: UInt64) async throws -> [WorkspaceChatEvent] {
        do { return try await direct.events(workspaceID: workspaceID, after: after) }
        catch { guard canSwitch(error) else { throw error }; return try await fallback().events(workspaceID: workspaceID, after: after) }
    }
    public func configure(workspaceID: String, revision: UInt64, change: WorkspaceConfigurationChange) async throws {
        // Select the transport using a read. Never replay an uncertain configuration mutation.
        let api: any WorkspaceChatAPI
        do { _ = try await direct.catalog(); api = direct }
        catch { guard canSwitch(error) else { throw error }; api = try await fallback() }
        try await api.configure(workspaceID: workspaceID, revision: revision, change: change)
    }
    public func invalidateRelay() async { let previous = relay; relay = nil; await previous?.close() }
    private func canSwitch(_ error: any Error) -> Bool {
        // Authorization/protocol/identity failures are never bypassed via another transport.
        error as? MobileError == .connectionFailed || error as? MobileError == .timedOut
    }
}

public struct DirectWorkspaceChatAPI: WorkspaceChatAPI {
    let machine: PairedMachine
    let client: MobileClient
    public init(machine: PairedMachine, client: MobileClient) { self.machine = machine; self.client = client }
    public func catalog() async throws -> WorkspaceCatalog {
        let reply = try await client.workspaceRPC(machine, type: "workspace_catalog")
        guard let result = reply.result?.workspaceCatalog, result.macID == machine.id else { throw MobileError.invalidResponse }
        return result
    }
    public func receipt(for message: WorkspaceChatMessage) async throws -> WorkspaceChatReceipt? {
        let fields = WorkspaceReceiptQuery(workspaceID: message.workspaceID, operationID: message.operationID)
        let reply = try await client.workspaceRPC(machine, type: "workspace_receipt", fields: JSONEncoder().encode(fields))
        try check(reply.result?.workspaceReceipt, message: message, macID: machine.id)
        return reply.result?.workspaceReceipt
    }
    public func submit(_ message: WorkspaceChatMessage) async throws -> WorkspaceChatReceipt {
        let reply = try await client.workspaceRPC(machine, type: "workspace_message", fields: JSONEncoder().encode(WorkspaceSubmitQuery(message: message)))
        guard let receipt = reply.result?.workspaceReceipt else { throw MobileError.invalidResponse }
        try check(receipt, message: message, macID: machine.id); return receipt
    }
    public func events(workspaceID: String, after: UInt64) async throws -> [WorkspaceChatEvent] {
        let reply = try await client.workspaceRPC(machine, type: "workspace_events", fields: JSONEncoder().encode(WorkspaceEventsQuery(workspaceID: workspaceID, after: after)))
        guard let events = reply.result?.workspaceEvents else { throw MobileError.invalidResponse }; return events
    }
    public func configure(workspaceID: String, revision: UInt64, change: WorkspaceConfigurationChange) async throws {
        let reply = try await client.workspaceRPC(machine, type: "workspace_configure", fields: JSONEncoder().encode(WorkspaceConfigureQuery(workspaceID: workspaceID, revision: revision, change: change)))
        guard reply.result?.status == "workspace_configured" else { throw MobileError.invalidResponse }
    }
}

extension MobileClient {
    func workspaceRPC(_ machine: PairedMachine, type: String, fields: Data = Data("{}".utf8)) async throws -> WireReply {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id), var action = try JSONSerialization.jsonObject(with: fields) as? [String: Any] else { throw MobileError.invalidCredential }
        action["type"] = type; action["device_id"] = credential.deviceID; action["token"] = credential.token
        return try await exchange(machine, action: action, timeout: .seconds(30))
    }
}

public struct RelayWorkspaceChatAPI: WorkspaceChatAPI {
    let connection: CollaborationRelayConnection
    public init(connection: CollaborationRelayConnection) { self.connection = connection }
    public func close() async { await connection.close() }
    public func catalog() async throws -> WorkspaceCatalog {
        let result = try await connection.workspaceRPC(type: "workspace_catalog").result?.workspaceCatalog
        guard let result, result.macID == connection.machineID else { throw MobileError.invalidResponse }; return result
    }
    public func receipt(for message: WorkspaceChatMessage) async throws -> WorkspaceChatReceipt? {
        let query = WorkspaceReceiptQuery(workspaceID: message.workspaceID, operationID: message.operationID)
        let reply = try await connection.workspaceRPC(type: "workspace_receipt", fields: JSONEncoder().encode(query))
        try check(reply.result?.workspaceReceipt, message: message, macID: connection.machineID); return reply.result?.workspaceReceipt
    }
    public func submit(_ message: WorkspaceChatMessage) async throws -> WorkspaceChatReceipt {
        let reply = try await connection.workspaceRPC(type: "workspace_message", fields: JSONEncoder().encode(WorkspaceSubmitQuery(message: message)))
        guard let receipt = reply.result?.workspaceReceipt else { throw MobileError.invalidResponse }
        try check(receipt, message: message, macID: connection.machineID); return receipt
    }
    public func events(workspaceID: String, after: UInt64) async throws -> [WorkspaceChatEvent] {
        let query = WorkspaceEventsQuery(workspaceID: workspaceID, after: after)
        let reply = try await connection.workspaceRPC(type: "workspace_events", fields: JSONEncoder().encode(query))
        guard let events = reply.result?.workspaceEvents else { throw MobileError.invalidResponse }; return events
    }
    public func configure(workspaceID: String, revision: UInt64, change: WorkspaceConfigurationChange) async throws {
        let query = WorkspaceConfigureQuery(workspaceID: workspaceID, revision: revision, change: change)
        let reply = try await connection.workspaceRPC(type: "workspace_configure", fields: JSONEncoder().encode(query))
        guard reply.result?.status == "workspace_configured" else { throw MobileError.invalidResponse }
    }
}
extension CollaborationRelayConnection {
    func workspaceRPC(type: String, fields: Data = Data("{}".utf8)) async throws -> WireReply {
        guard capabilities.contains("workspace_chat_v1") else { throw MobileError.unsupportedVersion }
        guard let values = try JSONSerialization.jsonObject(with: fields) as? [String: Any] else { throw MobileError.invalidResponse }
        return try await exchange(type: type, fields: values)
    }
}
private struct WorkspaceReceiptQuery: Encodable {
    let workspaceID: String; let operationID: String
    enum CodingKeys: String, CodingKey { case workspaceID = "desktop_workspace_id", operationID = "operation_id" }
}
private struct WorkspaceSubmitQuery: Encodable { let message: WorkspaceChatMessage }
private struct WorkspaceEventsQuery: Encodable {
    let workspaceID: String; let after: UInt64
    enum CodingKeys: String, CodingKey { case workspaceID = "desktop_workspace_id", after }
}
private struct WorkspaceConfigureQuery: Encodable {
    let workspaceID: String; let revision: UInt64; let change: WorkspaceConfigurationChange
    enum CodingKeys: String, CodingKey { case workspaceID = "desktop_workspace_id", revision = "expected_revision", change }
}
private func check(_ receipt: WorkspaceChatReceipt?, message: WorkspaceChatMessage, macID: String) throws {
    guard let receipt else { return }
    guard receipt.macID == macID, receipt.workspaceID == message.workspaceID, receipt.operationID == message.operationID,
          receipt.cursor > 0 else { throw MobileError.invalidResponse }
}

private struct ActivityQuery:Encodable {let request:WorkspaceActivityRequest}
extension MobileClient {
    /// Capture the existing credential in memory before forgetting; clear after any in-flight enrollment.
    public func prepareWorkspaceActivityRemoval(_ machine:PairedMachine) async throws -> @Sendable () async throws -> Void {
        try machine.validate()
        guard let credential=try await vault.load(machineID:machine.id) else {throw MobileError.invalidCredential}
        return {
            func query(_ request:WorkspaceActivityRequest) async throws -> WorkspaceActivitySubscription {
                let fields=try JSONSerialization.jsonObject(with:JSONEncoder().encode(request))
                let reply=try await exchange(machine,action:["type":"workspace_activity","device_id":credential.deviceID,"token":credential.token,"request":fields])
                guard let state=try activityResponse(reply,request:request,machine:machine.id).subscription else {throw MobileError.invalidResponse}
                return state
            }
            let state=try await query(.init(kind:"subscription"))
            if state.registration != nil {
                guard state.revision<Int64.max else {throw MobileError.invalidResponse}
                var removal=WorkspaceActivityRequest(kind:"set_subscription");removal.expectedRevision=state.revision
                do {
                    let cleared=try await query(removal)
                    guard cleared.registration==nil,cleared.revision==state.revision+1 else {throw MobileError.invalidResponse}
                } catch {
                    let original=error
                    guard let cleared=try? await query(.init(kind:"subscription")),cleared.registration==nil,cleared.revision==state.revision+1 else {throw original}
                }
            }
        }
    }
}
extension DirectWorkspaceChatAPI {
    public func activity(_ request:WorkspaceActivityRequest) async throws -> WorkspaceActivityResponse {
        let reply=try await client.workspaceRPC(machine,type:"workspace_activity",fields:JSONEncoder().encode(ActivityQuery(request:request)))
        return try activityResponse(reply,request:request,machine:machine.id)
    }
}
extension RelayWorkspaceChatAPI {
    public func activity(_ request:WorkspaceActivityRequest) async throws -> WorkspaceActivityResponse {
        let reply=try await connection.workspaceRPC(type:"workspace_activity",fields:JSONEncoder().encode(ActivityQuery(request:request)))
        return try activityResponse(reply,request:request,machine:connection.machineID)
    }
}
private func activityResponse(_ reply:WireReply,request:WorkspaceActivityRequest,machine:String) throws -> WorkspaceActivityResponse {
    guard reply.result?.status=="workspace_activity",let response=reply.result?.workspaceActivity else {throw MobileError.invalidResponse}
    if let summary=response.summary {
        guard summary.machineID==machine,summary.workspaceID==request.workspaceID,summary.attention.count<=4096 else {throw MobileError.invalidResponse}
        try summary.content.validate();for item in summary.attention {try item.target.validate()}
    }
    if let subscription=response.subscription {
        guard subscription.revision<=Int64.max else {throw MobileError.invalidResponse}
        try subscription.registration?.validate()
    }
    if request.kind=="detail" {
        guard response.target==request.target else {throw MobileError.invalidResponse}
        if let approval=response.approval {try approval.validate();guard request.target?.matches(approval.target)==true else {throw MobileError.invalidResponse}}
    }
    if let receipt=response.receipt {
        guard receipt.operationID==request.operationID,let area=request.target?.areaID ?? request.areaID,receipt.workspaceID=="desktop:\(request.workspaceID):\(area)",["accepted","succeeded","rejected","outcome_unknown"].contains(receipt.state) else {throw MobileError.invalidResponse}
        if receipt.state=="succeeded",let target=request.target {
            guard receipt.result?.nodeID==target.nodeID,receipt.result?.runID==target.runID,receipt.result?.sessionID==target.sessionID,receipt.result?.turnID==target.turnID else {throw MobileError.invalidResponse}
            if request.kind=="decide" {guard receipt.result?.requestID==target.requestID,receipt.result?.decision==request.decision else {throw MobileError.invalidResponse}}
            if request.kind=="accept" {guard receipt.result?.fingerprint==target.fingerprint else {throw MobileError.invalidResponse}}
        }
    }
    return response
}
