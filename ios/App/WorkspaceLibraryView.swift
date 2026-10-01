import SwiftUI
import UIKit
import NormaCore

@MainActor @Observable final class WorkspaceLibraryModel {
    let store: CollaborationWorkspaceStore
    private(set) var workspaces: [CollaborationWorkspace] = []
    private(set) var threads: [String: [WorkThread]] = [:]
    private(set) var messageDrafts: [String: WorkThreadDraft] = [:]
    private(set) var draftsLoaded = false
    private(set) var savingDrafts = false
    private(set) var draftErrors: [String: String] = [:]
    private var draftWrite: Task<Void, Never>?
    private var pendingDraftWrites: Set<String> = []
    private var backgroundWrite: UIBackgroundTaskIdentifier = .invalid
    private var backgroundGeneration = UUID()
    private(set) var loaded = false
    private(set) var saving = false
    var deliveryInProgress = false
    var deliveryMachineID: String?
    var terminalSessions: [String: UUID] = [:]
    var deliveryRevision = 0
    var deliveryErrors: [String: String] = [:]
    var taskDeliveryErrors: [String: String] = [:]
    var deliveryError: String?
    var error: String?

    init(store: CollaborationWorkspaceStore) { self.store = store }

    init() {
        var directory = URL.applicationSupportDirectory.appendingPathComponent("Norma/Workspaces")
        #if DEBUG
        if let namespace = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_TEST_NAMESPACE"], UUID(uuidString: namespace) != nil {
            directory = URL.applicationSupportDirectory.appendingPathComponent("Norma/UITestWorkspaces/\(namespace)")
        }
        #endif
        store = CollaborationWorkspaceStore(databaseURL: directory.appendingPathComponent("workspaces.sqlite"))
    }
    func load() async {
        do {
            if !draftsLoaded {
                let restored = try await store.messageDrafts()
                if !draftsLoaded { messageDrafts = restored; draftsLoaded = true }
            }
            let records = try await store.all()
            var histories: [String: [WorkThread]] = [:]
            for workspace in records { histories[workspace.id] = try await store.threads(workspaceID: workspace.id) }
            workspaces = records; threads = histories
            loaded = true; error = nil
        }
        catch { self.error = error.localizedDescription }
    }

    func updateMessageDraft(_ text: String, threadID: String) {
        guard draftsLoaded, (messageDrafts[threadID]?.text ?? "") != text else { return }
        let mentions = (messageDrafts[threadID]?.mentions ?? []).filter { ThreadMentions.contains($0, in: text) }
        let target = messageDrafts[threadID]?.followupTaskID
        messageDrafts[threadID] = text.isEmpty && target == nil ? nil : WorkThreadDraft(text: text, mentions: mentions, followupTaskID: target)
        persistDraft(threadID)
    }
    func mentionAgent(_ agent: WorkspaceAgentReference, threadID: String) {
        guard draftsLoaded else { return }
        var text = messageDrafts[threadID]?.text ?? ""
        if !ThreadMentions.contains(agent, in: text) {
            text = insertingMention(agent.name, into: text)
        }
        var mentions = (messageDrafts[threadID]?.mentions ?? []).filter { $0.name.caseInsensitiveCompare(agent.name) != .orderedSame && ThreadMentions.contains($0, in: text) }
        mentions.append(agent)
        messageDrafts[threadID] = WorkThreadDraft(text: text, mentions: mentions, followupTaskID: messageDrafts[threadID]?.followupTaskID)
        persistDraft(threadID)
    }
    func mentionNorma(threadID: String) {
        guard draftsLoaded else { return }
        let text = messageDrafts[threadID]?.text ?? ""
        if !ThreadMentions.requestsNorma(text) { updateMessageDraft(insertingMention("Norma", into: text), threadID: threadID) }
    }
    private func insertingMention(_ name: String, into original: String) -> String {
        var text = original
        if let partial = text.range(of: "(?<![\\p{L}\\p{N}_@])@[^\\s@，,。.!?！？:：;；]*$", options: .regularExpression) {
            text.replaceSubrange(partial, with: "@\(name) ")
        } else { text += (text.isEmpty || text.last?.isWhitespace == true ? "" : " ") + "@\(name) " }
        return text
    }
    func beginFollowup(operationID: String, threadID: String) async throws {
        guard draftsLoaded else { throw WorkThreadError.damagedStore }
        let parent = try await store.threadTask(operationID: operationID)
        guard parent.task.threadID == threadID else { throw WorkThreadError.invalidContent }
        guard parent.receipt?.receipt.state == "succeeded", parent.receipt?.receipt.result?.taskRef != nil else {
            throw ThreadTaskError.invalidTask
        }
        guard workspaces.first(where: { $0.id == parent.task.collaborationWorkspaceID })?.archived == false else {
            throw WorkThreadError.archived
        }
        let draft = messageDrafts[threadID]
        messageDrafts[threadID] = WorkThreadDraft(text: draft?.text ?? "", mentions: draft?.mentions ?? [], followupTaskID: operationID)
        persistDraft(threadID)
        try await flushMessageDrafts(threadID: threadID)
    }
    func cancelFollowup(threadID: String) {
        guard draftsLoaded, let draft = messageDrafts[threadID], draft.followupTaskID != nil else { return }
        messageDrafts[threadID] = draft.text.isEmpty ? nil : WorkThreadDraft(text: draft.text, mentions: draft.mentions ?? [])
        persistDraft(threadID)
    }
    func retryDraftSave(threadID: String) { if draftsLoaded { persistDraft(threadID) } }

    private func persistDraft(_ threadID: String) {
        pendingDraftWrites.insert(threadID); savingDrafts = true; draftErrors[threadID] = nil
        guard draftWrite == nil else { return }
        draftWrite = Task {
            // Coalesce edits while SQLite is writing, as ChatDraftBook does for legacy chat drafts.
            while let id = pendingDraftWrites.first {
                pendingDraftWrites.remove(id)
                let snapshot = messageDrafts[id]
                do {
                    try await store.saveMessageDraft(snapshot, threadID: id)
                    if !pendingDraftWrites.contains(id) { draftErrors[id] = nil }
                } catch {
                    if !pendingDraftWrites.contains(id) {
                        draftErrors[id] = (snapshot?.text.count ?? 0) > 100_000
                            ? "这条草稿超过 100,000 字，尚未保存。请缩短后重试，原文仍在输入框中。"
                            : "这条草稿尚未保存，请保留当前页面并重试。"
                    }
                }
            }
            savingDrafts = false; draftWrite = nil
        }
    }

    func flushMessageDrafts(threadID: String? = nil) async throws {
        guard draftsLoaded else { throw WorkThreadError.damagedStore }
        while let write = draftWrite { await write.value }
        if threadID.map({ draftErrors[$0] != nil }) ?? !draftErrors.isEmpty { throw WorkThreadError.damagedStore }
    }

    func sendMessage(threadID: String, text: String, cloud: CloudChatModel? = nil) async throws -> WorkThreadMessage {
        let draft = messageDrafts[threadID].flatMap { $0.text == text ? $0 : nil }
        let revision = draft?.revision
        try await flushMessageDrafts(threadID: threadID)
        let message: WorkThreadMessage
        if let target = draft?.followupTaskID {
            let parent = try await store.threadTask(operationID: target)
            guard parent.task.threadID == threadID else { throw WorkThreadError.invalidContent }
            let messageID = UUID().uuidString
            _ = try await store.appendThreadFollowup(parentOperationID: target, messageID: messageID,
                text: text, operationID: UUID().uuidString, clearingDraftRevision: revision)
            message = try await store.sourceMessage(id: messageID, workspaceID: parent.task.collaborationWorkspaceID)
        } else if ThreadMentions.requestsNorma(text) {
            guard let cloud else { throw ThreadMentionError.normaUnavailable }
            let prepared = try await store.prepareNormaTurn(threadID: threadID, messageID: UUID().uuidString,
                text: text, selected: draft?.mentions ?? [], clearingDraftRevision: revision)
            let response = try await cloud.planThread(prepared.request)
            try Task.checkCancellation()
            message = try await store.applyNormaTurn(response, prepared: prepared)
        } else {
            message = try await store.postThreadMessage(threadID: threadID, id: UUID().uuidString, text: text,
                selected: draft?.mentions ?? [], clearingDraftRevision: revision)
        }
        if let revision, messageDrafts[threadID]?.revision == revision {
            messageDrafts[threadID] = nil; persistDraft(threadID)
        }
        return message
    }

    func finishDraftWrites() {
        guard savingDrafts, backgroundWrite == .invalid else { return }
        let generation = UUID(); backgroundGeneration = generation
        backgroundWrite = UIApplication.shared.beginBackgroundTask(withName: "Save work thread drafts") { [weak self] in
            Task { @MainActor in self?.endDraftWrites(generation) }
        }
        Task {
            try? await flushMessageDrafts() // Unsaved text and per-thread errors remain available for retry.
            endDraftWrites(generation)
        }
    }
    private func endDraftWrites(_ generation: UUID) {
        guard generation == backgroundGeneration, backgroundWrite != .invalid else { return }
        UIApplication.shared.endBackgroundTask(backgroundWrite); backgroundWrite = .invalid
    }
    func create(_ name: String) async -> CollaborationWorkspace? {
        guard !saving else { return nil }
        saving = true
        defer { saving = false }
        do {
            let record = try await store.create(name: name)
            await load()
            return record
        } catch { self.error = error.localizedDescription; return nil }
    }
    func save(_ workspace: CollaborationWorkspace) async -> Bool {
        guard !saving else { return false }
        saving = true
        defer { saving = false }
        do { try await store.save(workspace); await load(); return true }
        catch { self.error = error.localizedDescription; return false }
    }
}

struct WorkspaceLibraryView: View {
    @Bindable var library: WorkspaceLibraryModel
    let computers: ComputerModel
    let openIdeas: () -> Void
    let openComputers: () -> Void
    @Environment(\.workspaceNotifications) private var openSettings
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var drawer = false
    @State private var creating = false
    @State private var name = ""
    @State private var search = ""
    @State private var archived = false
    @State private var selected: CollaborationWorkspace?
    @State private var configuring: CollaborationWorkspace?
    private var visible: [CollaborationWorkspace] {
        library.workspaces.filter { $0.archived == archived && (search.isEmpty || $0.name.localizedStandardContains(search)) }
    }
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Button { drawer = true } label: {
                    Image(systemName: "line.3.horizontal").font(.title3).frame(width: 48, height: 48)
                        .background(IdeaPalette.softGray, in: Circle())
                }.accessibilityLabel("更多模块").accessibilityIdentifier("workspace-more-modules")
                let controls = typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading, spacing: 12)) : AnyLayout(HStackLayout())
                controls {
                    Text("工作空间").font(.largeTitle.bold())
                    if !typeSize.isAccessibilitySize { Spacer() }
                    Button { name = ""; creating = true } label: {
                        Label("新建", systemImage: "plus").font(.headline).padding(.horizontal, 16).frame(minHeight: 48)
                            .foregroundStyle(.white).background(IdeaPalette.blue, in: RoundedRectangle(cornerRadius: 14))
                    }.accessibilityLabel("新建工作空间").accessibilityIdentifier("create-workspace")
                }
                Text("连接项目与 Agent，把想法变成成果。")
                    .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                controls {
                    HStack {
                        Image(systemName: "magnifyingglass")
                        TextField("搜索工作空间", text: $search).accessibilityIdentifier("workspace-search")
                    }.frame(minHeight: 44)
                    Menu {
                        Picker("状态", selection: $archived) {
                            Text("活动中").tag(false)
                            Text("已归档").tag(true)
                        }
                    } label: {
                        Label(archived ? "已归档" : "活动中", systemImage: "line.3.horizontal.decrease")
                            .fixedSize(horizontal: true, vertical: false).frame(minHeight: 44).contentShape(Rectangle())
                    }
                        .accessibilityIdentifier("workspace-archive-filter")
                }.font(.subheadline).padding(.horizontal, 14).padding(.vertical, 6)
                    .background(.white, in: RoundedRectangle(cornerRadius: 18))
                if !library.loaded { ProgressView("正在读取工作空间…") }
                else if visible.isEmpty {
                    ContentUnavailableView(archived ? "没有已归档的工作空间" : "还没有工作空间", systemImage: "folder",
                                           description: Text("想法独立保存在想法库。准备推进时，创建工作空间并关联项目。"))
                }
                ForEach(visible) { workspace in
                    Button { selected = workspace } label: {
                        VStack(alignment: .leading, spacing: 14) {
                            HStack(spacing: 12) {
                                Image(systemName: "folder.fill").font(.title2).foregroundStyle(IdeaPalette.green)
                                    .frame(width: 48, height: 48).background(IdeaPalette.mint.opacity(0.2), in: RoundedRectangle(cornerRadius: 15))
                                VStack(alignment: .leading, spacing: 5) {
                                    Text(workspace.name).font(.title3.bold())
                                    Text("\(workspace.projects.count) 个项目 · \(workspace.agents.count + 1) 位 Agent · \(library.threads[workspace.id]?.count ?? 0) 件事项")
                                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                                }
                                Spacer()
                                Image(systemName: "chevron.right").foregroundStyle(IdeaPalette.secondary)
                            }
                            if workspace.projects.isEmpty {
                                Text("添加项目和 Agent，准备开始协作").font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                            } else {
                                Text(workspace.projects.map(\.name).joined(separator: " · "))
                                    .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                            }
                            if let recent = library.threads[workspace.id]?.first {
                                Label(recent.title, systemImage: "doc.text")
                                    .font(.subheadline.weight(.medium)).lineLimit(2)
                                    .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                                    .background(IdeaPalette.blue.opacity(0.06), in: RoundedRectangle(cornerRadius: 14))
                            }
                            HStack(spacing: 10) {
                                Text("参与的 Agent").font(.caption).foregroundStyle(IdeaPalette.secondary)
                                Image("NormaPortrait").resizable().scaledToFill().frame(width: 30, height: 30).clipShape(Circle())
                                    .accessibilityLabel("Norma，协调者")
                                Text(workspace.agents.isEmpty ? "Norma" : workspace.agents.map(\.name).joined(separator: " · "))
                                    .font(.caption).lineLimit(2)
                                Spacer(minLength: 44)
                            }.frame(minHeight: 44)
                        }.padding(18).frame(maxWidth: .infinity, alignment: .leading)
                            .background(.white, in: RoundedRectangle(cornerRadius: 24))
                    }.accessibilityIdentifier("workspace-card-\(workspace.id)")
                        .overlay(alignment: .bottomTrailing) {
                            Button { configuring = workspace } label: {
                                Image(systemName: "plus").frame(width: 44, height: 44).background(IdeaPalette.softGray, in: Circle())
                            }.accessibilityLabel("配置 \(workspace.name) 的项目与 Agent")
                                .accessibilityIdentifier("workspace-configure-\(workspace.id)")
                                .padding(18)
                        }
                }
            }.padding(20)
        }.background(IdeaPalette.background).foregroundStyle(IdeaPalette.ink).buttonStyle(IdeaPressStyle())
            .task { await library.load() }
            .sheet(isPresented: $drawer) {
                NavigationStack {
                    List {
                        Button { drawer = false; openIdeas() } label: { Label("想法库", systemImage: "lightbulb") }
                        Button { drawer = false; openComputers() } label: { Label("已连接电脑", systemImage: "laptopcomputer") }
                        Button { drawer = false; openSettings() } label: { Label("设置", systemImage: "gearshape") }
                    }.navigationTitle("更多模块").navigationBarTitleDisplayMode(.inline)
                        .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { drawer = false } } }
                }.presentationDetents([.medium]).presentationDragIndicator(.visible)
            }
            .sheet(isPresented: $creating) {
                NavigationStack {
                    Form {
                        TextField("工作空间名称", text: $name).accessibilityIdentifier("workspace-name")
                        Text("想法不会自动加入；项目与 Agent 可在创建后设置。")
                            .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                    }.navigationTitle("新建工作空间").navigationBarTitleDisplayMode(.inline)
                        .toolbar {
                            ToolbarItem(placement: .cancellationAction) { Button("取消") { creating = false } }
                            ToolbarItem(placement: .confirmationAction) {
                                Button("创建") {
                                    Task {
                                        if await library.create(name) != nil { creating = false }
                                    }
                                }.disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || library.saving)
                                    .accessibilityIdentifier("workspace-create-save")
                            }
                        }
                }.presentationDetents([.medium]).presentationDragIndicator(.visible)
            }
            .fullScreenCover(item: $configuring) { workspace in
                WorkspaceSettingsView(library: library, computers: computers, workspace: workspace)
            }
            .fullScreenCover(item: $selected, onDismiss: { Task { await library.load() } }) { workspace in
                WorkspaceThreadListView(library: library, computers: computers, workspaceID: workspace.id)
            }
            .alert("工作空间", isPresented: Binding(get: { library.error != nil }, set: { if !$0 { library.error = nil } })) {
                Button("好") { library.error = nil }
            } message: { Text(library.error ?? "") }
    }
}
