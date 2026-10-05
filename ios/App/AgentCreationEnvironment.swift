import Foundation
import NormaCore

/// UI and background App Intent share the same actor and durable operation book.
@MainActor final class AgentCreationEnvironment {
    static let shared = AgentCreationEnvironment()
    let store: WorkspaceChatStore
    let service: AgentCreationService
    private let machines = MachineStore(url: URL.applicationSupportDirectory.appendingPathComponent("Norma/machines.json"))
    private let vault = CredentialVault()

    init() {
        var directory = URL.applicationSupportDirectory.appendingPathComponent("Norma/WorkspaceChat")
        #if DEBUG
        if let namespace = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_TEST_NAMESPACE"], UUID(uuidString: namespace) != nil {
            directory = URL.applicationSupportDirectory.appendingPathComponent("Norma/UITestWorkspaceChat/\(namespace)")
        }
        #endif
        store = WorkspaceChatStore(databaseURL: directory.appendingPathComponent("chat.sqlite"))
        service = AgentCreationService(store: store)
    }
    func catalogs() async throws -> [(PairedMachine, WorkspaceCatalog)] {
        var result: [(PairedMachine, WorkspaceCatalog)] = []
        for machine in try await machines.load() {
            if let catalog = try await store.catalog(macID: machine.id) { result.append((machine, catalog)) }
        }
        return result
    }
    func client(macID: String) async throws -> WorkspaceChatClient {
        guard let machine = try await machines.load().first(where: { $0.id == macID }) else { throw AgentCreationError.changed }
        let vault = vault
        return WorkspaceChatClient(direct: DirectWorkspaceChatAPI(machine: machine, client: MobileClient(vault: vault))) {
            let cloud = try await Self.cloudVault().load()
            guard let cloud else { throw AgentCreationError.offline }
            return try await RelayWorkspaceChatAPI(connection: collaborationRelayClient().connect(machine, cloud: cloud, vault: vault))
        }
    }
    func create(macID: String, workspaceID: String, areaID: String, profile: String) async throws -> AgentCreationReceipt {
        let client = try await client(macID: macID)
        do {
            let receipt = try await service.create(macID: macID, workspaceID: workspaceID, areaID: areaID, profile: profile, client: client)
            // Creation is already proven by the receipt. A failed refresh cannot turn it into a retry.
            if let catalog = try? await client.catalog() { try? await store.saveCatalog(catalog) }
            await client.invalidateRelay()
            return receipt
        } catch { await client.invalidateRelay(); throw error }
    }
    private static func cloudVault() throws -> CloudCredentialVault {
        var service = "com.norma-ai.os.ios.cloud"
        if let local = try LocalCloudConfiguration.bundled() { service += ".local.\(local.namespace).\(local.certificateSHA256)" }
        #if DEBUG
        if let namespace = ProcessInfo.processInfo.environment["NORMA_CLOUD_TEST_NAMESPACE"], UUID(uuidString: namespace) != nil { service += ".\(namespace)" }
        if let local = try CloudTextClient.developmentSettings(in: ProcessInfo.processInfo.environment) {
            guard try LocalCloudConfiguration.bundled() == nil else { throw CloudTextError.invalidRequest }
            service = "com.norma-ai.os.ios.cloud.local.\(local.port).\(local.namespace)"
        }
        #endif
        return CloudCredentialVault(service: service)
    }
}
