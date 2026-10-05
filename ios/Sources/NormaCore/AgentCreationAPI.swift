import Foundation

extension DirectWorkspaceChatAPI {
    public func createAgent(_ request: WorkspaceAgentCreateRequest) async throws -> AgentCreationReceipt {
        let reply = try await client.workspaceRPC(machine, type: "workspace_agent_create", fields: JSONEncoder().encode(CreationSubmit(request: request)))
        guard let receipt = reply.result?.agentCreationReceipt else { throw MobileError.invalidResponse }
        try receipt.validate(request, macID: machine.id); return receipt
    }
    public func creationReceipt(_ request: WorkspaceAgentCreateRequest) async throws -> AgentCreationReceipt? {
        let reply = try await client.workspaceRPC(machine, type: "workspace_agent_creation_receipt", fields: JSONEncoder().encode(CreationQuery(workspaceID: request.workspaceID, operationID: request.operationID)))
        try reply.result?.agentCreationReceipt?.validate(request, macID: machine.id)
        return reply.result?.agentCreationReceipt
    }
}
extension RelayWorkspaceChatAPI {
    public func createAgent(_ request: WorkspaceAgentCreateRequest) async throws -> AgentCreationReceipt {
        guard await connection.capabilities.contains("workspace_agent_create_v1") else { throw AgentCreationError.unsupported }
        let reply = try await connection.workspaceRPC(type: "workspace_agent_create", fields: JSONEncoder().encode(CreationSubmit(request: request)))
        guard let receipt = reply.result?.agentCreationReceipt else { throw MobileError.invalidResponse }
        try receipt.validate(request, macID: connection.machineID); return receipt
    }
    public func creationReceipt(_ request: WorkspaceAgentCreateRequest) async throws -> AgentCreationReceipt? {
        guard await connection.capabilities.contains("workspace_agent_create_v1") else { throw AgentCreationError.unsupported }
        let reply = try await connection.workspaceRPC(type: "workspace_agent_creation_receipt", fields: JSONEncoder().encode(CreationQuery(workspaceID: request.workspaceID, operationID: request.operationID)))
        try reply.result?.agentCreationReceipt?.validate(request, macID: connection.machineID)
        return reply.result?.agentCreationReceipt
    }
}
private struct CreationSubmit: Encodable { let request: WorkspaceAgentCreateRequest }
private struct CreationQuery: Encodable {
    let workspaceID: String
    let operationID: String
    enum CodingKeys: String, CodingKey { case workspaceID = "desktop_workspace_id", operationID = "operation_id" }
}
