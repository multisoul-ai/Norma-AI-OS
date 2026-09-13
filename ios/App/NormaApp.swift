import SwiftUI
import UIKit
import NormaCore

@main struct NormaApp: App {
    @UIApplicationDelegateAdaptor(PushNotifications.self) private var notifications
    @State private var model = ComputerModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView(model: model, notifications: notifications)
                .background(KeyboardDismissal().frame(width: 0, height: 0).accessibilityHidden(true))
                .preferredColorScheme(.dark)
                .tint(.white)
                .task {
                    await model.load()
                    notifications.tokenChanged = { [weak model, weak notifications] in
                        guard let model, let notifications else { return }
                        Task { await notifications.synchronize(model.machines, client: model.client) }
                    }
                    await notifications.refreshSystem()
                }
                .onChange(of: scenePhase) { _, phase in
                    if phase == .background { model.disconnect(); model.finishDraftWrites() }
                    if phase == .active {
                        model.refresh()
                        Task {
                            await notifications.refreshSystem()
                            await notifications.synchronize(model.machines, client: model.client)
                        }
                    }
                }
        }
    }
}

@MainActor @Observable final class ComputerModel {
    var loaded = false
    var machines: [PairedMachine] = []
    var selectedID: String? = UserDefaults.standard.string(forKey: "selected-machine")
    var grant: DeviceGrant?
    var projects: [ProjectSnapshot]?
    private var projectSelections: [String: String] = [:]
    var commandShowsPlan = true
    var selectedProject: ProjectSnapshot? {
        guard let selectedID else { return nil }
        return projects?.first { $0.id == projectSelections[selectedID] } ?? projects?.first
    }
    func selectProject(_ id: String) {
        guard let selectedID, projects?.contains(where: { $0.id == id }) == true else { return }
        projectSelections[selectedID] = id
    }
    var pendingCommands: [PendingMobileCommand] = []
    var acknowledgedQueueIDs: Set<String> = []
    var queueStatuses: [String: QueuedMessageStatus] = [:]
    var checkedAt: Date?
    var connecting = false
    var error: String?
    let vault = CredentialVault()
    let store = MachineStore(url: URL.applicationSupportDirectory.appendingPathComponent("Norma/machines.json"))
    let commandStore = PendingCommandStore(url: URL.applicationSupportDirectory.appendingPathComponent("Norma/commands.json"))
    let drafts = ChatDraftBook(store: ChatDraftStore(url: URL.applicationSupportDirectory.appendingPathComponent("Norma/drafts.json")))
    private var draftBackground: UIBackgroundTaskIdentifier = .invalid
    private var draftBackgroundGeneration = UUID()
    private var operation: Task<Void, Never>?
    var client: MobileClient { MobileClient(vault: vault) }
    var selected: PairedMachine? { machines.first { $0.id == selectedID } }

    func load() async {
        defer { loaded = true }
        await drafts.load()
        do { try await reloadCommands() }
        catch { self.error = "无法读取未决操作。原始记录已保留。" }
        do {
            machines = try await store.load()
            if selected == nil { select(machines.first?.id) }
            else { refresh() }
        } catch { self.error = "无法读取电脑列表。原始记录已保留。" }
    }
    func reloadCommands() async throws { pendingCommands = try await commandStore.load() }
    func confirmQueue(_ pending: PendingMobileCommand, receipt: MobileCommandReceipt) async throws {
        guard pending.request.command.isQueuedMessage, receipt.state == "succeeded" else { return }
        try await drafts.confirm(pending, receipt: receipt)
        acknowledgedQueueIDs.insert(pending.id)
    }
    func refreshQueue(_ pending: PendingMobileCommand, machine: PairedMachine) async throws {
        if !acknowledgedQueueIDs.contains(pending.id) {
            try await confirmQueue(pending, receipt: client.receipt(for: pending, on: machine))
        }
        queueStatuses[pending.id] = try await client.queuedMessage(pending, on: machine)
    }
    func finishDraftWrites() {
        guard drafts.saving, draftBackground == .invalid else { return }
        let generation = UUID(); draftBackgroundGeneration = generation
        draftBackground = UIApplication.shared.beginBackgroundTask(withName: "Save chat drafts") { [weak self] in
            Task { @MainActor in self?.endDraftWrites(generation) }
        }
        Task {
            try? await drafts.flush() // The draft book keeps the unsaved text and exposes any storage error.
            endDraftWrites(generation)
        }
    }
    private func endDraftWrites(_ generation: UUID) {
        guard generation == draftBackgroundGeneration, draftBackground != .invalid else { return }
        UIApplication.shared.endBackgroundTask(draftBackground); draftBackground = .invalid
    }
    func select(_ id: String?, verified: DeviceGrant? = nil, reportErrors: Bool = true) {
        disconnect()
        selectedID = id
        UserDefaults.standard.set(id, forKey: "selected-machine")
        if let verified { grant = verified; checkedAt = .now }
        else { refresh(reportErrors: reportErrors) }
    }
    func disconnect() {
        operation?.cancel(); operation = nil
        connecting = false; grant = nil; projects = nil; checkedAt = nil
    }
    func refresh() { refresh(reportErrors: true) }
    private func refresh(reportErrors: Bool) {
        disconnect()
        guard let machine = selected else { return }
        connecting = true
        operation = Task {
            do {
                let result = try await client.snapshot(machine)
                try Task.checkCancellation()
                guard selectedID == machine.id else { return }
                grant = result.grant; projects = result.projects; checkedAt = .now
            } catch {
                guard !Task.isCancelled else { return }
                if reportErrors { self.error = error.localizedDescription }
            }
            connecting = false
        }
    }
    func refreshAndWait() async { refresh(); await operation?.value }
    func remember(_ machine: PairedMachine) async throws {
        try await store.save(machine)
        machines = try await store.load()
    }
    func forget(_ machine: PairedMachine, notifications: PushNotifications? = nil) async {
        do {
            if selectedID == machine.id { disconnect(); error = nil }
            let cleanup = notifications == nil ? nil : try? await client.preparePushSubscriptionRemoval(machine)
            try await vault.remove(machineID: machine.id)
            try await store.remove(id: machine.id)
            notifications?.forget(machine.id, cleanup: cleanup)
            projectSelections[machine.id] = nil
            machines = try await store.load()
            if selectedID == machine.id { select(machines.first?.id, reportErrors: false) }
        } catch { self.error = error.localizedDescription }
    }
}
