import SwiftUI
import NormaCore

@MainActor @Observable final class WorkspaceChatModel {
    let store: WorkspaceChatStore
    private(set) var catalog: WorkspaceCatalog?
    private(set) var loaded = false
    private(set) var online = false
    private(set) var updating = false
    private(set) var synchronizing = false
    private(set) var outbox: [WorkspaceOutboxEntry] = []
    private(set) var events: [String: [WorkspaceChatEvent]] = [:]
    private(set) var drafts: [String: WorkspaceChatDraft] = [:]
    var activeWorkspaceID: String?
    var error: String?
    var messageErrors: [String: String] = [:]
    private var clients: [String: WorkspaceChatClient] = [:]
    private var clientMachines: [String: PairedMachine] = [:]
    private var pendingIndex = 0
    private(set) var historyWorkspaces: [DesktopChatWorkspace] = []
    private var loadedMacID: String?
    private var lastCatalogRead = Date.distantPast
    private var draftWrites: [String: Task<Void, Never>] = [:]
    private var syncIndex = 0
    private var draftSaveErrors: [String: String] = [:]

    init(store: WorkspaceChatStore? = nil) {
        self.store = store ?? AgentCreationEnvironment.shared.store
    }
    func reloadCreationCatalog(macID: String) async {
        guard loadedMacID == macID else { return }
        do { catalog = try await store.catalog(macID: macID); lastCatalogRead = .distantPast }
        catch { self.error = error.localizedDescription }
    }
    var workspaces: [DesktopChatWorkspace] { catalog?.workspaces ?? [] }
    var unavailableWorkspaces: [DesktopChatWorkspace] { historyWorkspaces.filter { old in !workspaces.contains { $0.id == old.id } } }
    func workspace(_ id: String) -> DesktopChatWorkspace? { workspaces.first { $0.id == id } ?? historyWorkspaces.first { $0.id == id } }
    func isAvailable(_ id: String) -> Bool { workspaces.contains { $0.id == id } }
    func messages(_ id: String) -> [WorkspaceOutboxEntry] { outbox.filter { $0.message.workspaceID == id } }

    func load(macID: String) async throws {
        if loadedMacID != macID {
            catalog = try await store.catalog(macID: macID); loadedMacID = macID
            events = [:]; drafts = [:]; loaded = true; online = false; lastCatalogRead = .distantPast
        }
        outbox = try await store.outbox(macID: macID)
        historyWorkspaces = try await store.historicalWorkspaces(macID: macID)
        for workspace in historyWorkspaces {
            events[workspace.id] = try await store.events(macID: macID, workspaceID: workspace.id)
            if drafts[workspace.id] == nil { drafts[workspace.id] = try await store.draftState(macID: macID, workspaceID: workspace.id) }
        }
    }
    private func client(for machine: PairedMachine, computers: ComputerModel, cloud: CloudChatModel) async -> WorkspaceChatClient {
        if let client = clients[machine.id], clientMachines[machine.id] == machine { return client }
        await clients[machine.id]?.invalidateRelay()
        let vault = computers.vault
        let client = WorkspaceChatClient(direct: DirectWorkspaceChatAPI(machine: machine, client: computers.client)) {
            await cloud.load()
            guard let credential = await cloud.credential else { throw WorkspaceChatError.unavailable }
            let relay = try await collaborationRelayClient().connect(machine, cloud: credential, vault: vault)
            return RelayWorkspaceChatAPI(connection: relay)
        }
        clients[machine.id] = client; clientMachines[machine.id] = machine; return client
    }
    func activity(machineID: String, request: WorkspaceActivityRequest, computers: ComputerModel, cloud: CloudChatModel) async throws -> WorkspaceActivityResponse {
        guard let machine=computers.machines.first(where:{$0.id==machineID}) else {throw MobileError.wrongComputer}
        return try await client(for:machine,computers:computers,cloud:cloud).activity(request)
    }
    func synchronize(computers: ComputerModel, cloud: CloudChatModel, refresh: Bool = false) async {
        guard !synchronizing, let machine = computers.selected ?? computers.machines.first else { loaded = true; online = false; return }
        synchronizing = true
        defer { synchronizing = false }
        do {
            try await load(macID: machine.id)
            let client = await client(for: machine, computers: computers, cloud: cloud)
            if refresh || Date.now.timeIntervalSince(lastCatalogRead) >= 30 {
                do {
                    let remote = try await client.catalog()
                    try await store.saveCatalog(remote)
                    catalog = remote; lastCatalogRead = .now; online = true; error = nil
                    try await load(macID: machine.id)
                } catch {
                    online = false; self.error = error.localizedDescription
                    // Cached identities still allow receipt lookup; authorization failures remain terminal.
                    if error as? MobileError != .connectionFailed && error as? MobileError != .timedOut { throw error }
                }
            }
            let pending = try await store.pending(macID: machine.id).filter { isAvailable($0.message.workspaceID) }
            let batch = pending.isEmpty ? [] : Array((pending + pending).dropFirst(pendingIndex % pending.count).prefix(min(2,pending.count)))
            pendingIndex += batch.count
            for pending in batch {
                do { let receipt = try await client.deliver(pending.message); try await store.apply(receipt); messageErrors[pending.id] = nil }
                catch is CancellationError { throw CancellationError() }
                catch { messageErrors[pending.id] = error.localizedDescription }
            }
            let ids = workspaces.map(\.id)
            let background = ids.isEmpty ? [] : [ids[syncIndex % ids.count]]
            syncIndex += 1
            let scopes = Set([activeWorkspaceID].compactMap { $0 }.filter(isAvailable) + background)
            for id in scopes {
                let after = try await store.cursor(macID: machine.id, workspaceID: id)
                let updates = try await client.events(workspaceID: id, after: after)
                try await store.applyEvents(updates, macID: machine.id, workspaceID: id, after: after)
                events[id] = try await store.events(macID: machine.id, workspaceID: id)
            }
            outbox = try await store.outbox(macID: machine.id)
            online = true; error = nil
        } catch is CancellationError { return }
        catch {
            online = false; self.error = error.localizedDescription
            if let client = clients[machine.id] { await client.invalidateRelay() }
        }
    }
    func updateDraft(_ draft: WorkspaceChatDraft, workspaceID: String) {
        guard let macID = loadedMacID else { return }
        drafts[workspaceID] = draft
        let previous = draftWrites[workspaceID]
        draftWrites[workspaceID] = Task {
            await previous?.value
            do { try await store.saveDraft(draft, macID: macID, workspaceID: workspaceID); draftSaveErrors[workspaceID] = nil }
            catch { draftSaveErrors[workspaceID] = error.localizedDescription }
        }
    }
    func flushDrafts() async { for write in draftWrites.values { await write.value } }
    func send(workspaceID: String, intent: WorkspaceMessageIntent, target: WorkspaceAgentTarget?, ideaID: String? = nil) async -> Bool {
        guard isAvailable(workspaceID), let macID = loadedMacID, let draft = drafts[workspaceID], !draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return false }
        await draftWrites[workspaceID]?.value
        guard draftSaveErrors[workspaceID] == nil else { error = draftSaveErrors[workspaceID]; return false }
        let message = WorkspaceChatMessage(workspaceID: workspaceID, text: draft.text, intent: intent, target: target, ideaID: ideaID)
        do {
            try await store.enqueue(message, macID: macID)
            if drafts[workspaceID] == draft { drafts[workspaceID] = .init() }
            outbox = try await store.outbox(macID: macID); return true
        } catch { self.error = error.localizedDescription; return false }
    }
    func checkReceipt(_ entry: WorkspaceOutboxEntry, computers: ComputerModel, cloud: CloudChatModel) async {
        guard let machine = computers.machines.first(where: { $0.id == entry.macID }) else { return }
        do {
            let receipt = try await client(for: machine, computers: computers, cloud: cloud).query(entry.message)
            if let receipt { try await store.apply(receipt) }
            outbox = try await store.outbox(macID: machine.id)
            messageErrors[entry.id] = nil
        } catch { messageErrors[entry.id] = error.localizedDescription }
    }
    func configure(workspaceID: String, change: WorkspaceConfigurationChange, computers: ComputerModel, cloud: CloudChatModel) async -> Bool {
        guard online, !updating, let machine = computers.selected ?? computers.machines.first else { error = "连接 Mac 后才能修改配置；未提交的输入会保留。"; return false }
        updating = true
        defer { updating = false }
        do {
            let client = await client(for: machine, computers: computers, cloud: cloud)
            try await client.configure(workspaceID: workspaceID, revision: workspace(workspaceID)?.revision ?? 0, change: change)
            let remote = try await client.catalog(); try await store.saveCatalog(remote); catalog = remote
            lastCatalogRead = .now; error = nil; return true
        } catch { self.error = error.localizedDescription; lastCatalogRead = .distantPast; return false }
    }
}
