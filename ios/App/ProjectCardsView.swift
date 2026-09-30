import SwiftUI
import NormaCore

struct ProjectCardsView: View {
    let machine: PairedMachine
    let workspaceID: String
    let name: String
    let model: ComputerModel
    var active = true
    var manageComputers: (() -> Void)? = nil
    var switchProject: (() -> Void)? = nil
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.dynamicTypeSize) private var typeSize
    @ScaledMetric(relativeTo: .headline) private var projectTitleSize = 20
    @State private var project: ProjectSnapshot?
    @State private var agentDefinitions: [ProjectAgentDefinition] = []
    @State private var loading = false
    @State private var error: String?
    @State private var generation = 0
    @State private var retry = 0
    @State private var search = ""
    @State private var showingFilters = false
    @State private var stateFilter: String?
    @State private var profileFilter: String?
    private var filtering: Bool { !search.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || stateFilter != nil || profileFilter != nil }
    private var visibleCards: [CardSnapshot] {
        project?.cards?.filter { $0.matchesSearch(search, state: stateFilter, profile: profileFilter) } ?? []
    }
    @State private var showingCreation = false
    @State private var creationRoute: String?
    @State private var options: CardSnapshot?
    @State private var conversation: (card: CardSnapshot, route: String)?
    @State private var showingConversation = false
    @State private var creatingProfile: String?
    @State private var creationWork: Task<Void, Never>?
    @State private var creationNotice: String?
    @State private var canCreate = false
    @State private var revision: UInt64 = 0

    private var pendingCreation: PendingMobileCommand? {
        model.pendingCommands.first {
            guard $0.machineID == machine.id, $0.request.workspaceID == workspaceID else { return false }
            if case .createCard = $0.request.command { return true }
            return false
        }
    }
    private var creationUnavailable: String? {
        if project?.state != "open" { return "项目暂时无法加载，请检查电脑连接后重试。" }
        if !canCreate { return "当前项目为只读，请在电脑上允许控制后创建。" }
        if pendingCreation != nil { return "上次创建结果待确认，请先查询结果。" }
        return nil
    }

    private var projectContent: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                projectHeading
                if let creatingProfile {
                    HStack(spacing: 8) { ProgressView(); Text("正在创建 \(creatingProfile)…").font(.caption) }
                        .frame(maxWidth: .infinity)
                        .accessibilityIdentifier("card-creation-progress")
                } else if let creationNotice {
                    Text(creationNotice).font(.caption).foregroundStyle(.secondary)
                        .accessibilityIdentifier("card-creation-status")
                }
                if let project, let cards = project.cards {
                    if cards.isEmpty {
                        WorkspaceStatusView(title: "项目中还没有 Agent 卡片", icon: "square.grid.2x2", message: "选择 Agent 类型创建卡片，再开始对话。", inline: true) {}
                    } else if visibleCards.isEmpty {
                        WorkspaceStatusView(title: "没有匹配的卡片", icon: "magnifyingglass", message: "试试其他任务名称、状态或 Agent。", inline: true) {
                            WorkspaceStatusButton(title: "清除筛选", action: clearFilters)
                                .accessibilityIdentifier("clear-card-filters")
                        }
                    } else {
                        if filtering {
                            HStack {
                                Text("\(visibleCards.count) / \(cards.count) 张卡片").foregroundStyle(.secondary)
                                Spacer()
                                Button("清除筛选", action: clearFilters).frame(minHeight: 44)
                                    .accessibilityIdentifier("clear-card-filters")
                            }.font(.caption)
                        }
                        HStack {
                            Menu {
                                Button("搜索卡片") { showingFilters = true }
                                cardFilters
                            } label: {
                                Text("全部 · \(cards.count)").font(.system(size: 11, weight: .medium))
                                    .padding(.horizontal, 12).frame(minHeight: 35)
                                    .background(WorkspaceStyle.raised, in: Capsule())
                                    .overlay(Capsule().strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                                    .frame(minHeight: 44)
                            }.accessibilityLabel("筛选卡片").accessibilityIdentifier("card-filters")
                            Spacer()
                            HStack(spacing: 5) {
                                Circle().fill(Color(white: 0.8)).frame(width: 5, height: 5)
                                Text("\(cards.filter { ["thinking", "running_tool"].contains($0.state) }.count) 张运行中")
                            }.font(.system(size: 11)).foregroundStyle(WorkspaceStyle.muted)
                                .padding(.horizontal, 12).frame(minHeight: 35)
                                .background(WorkspaceStyle.surface, in: Capsule())
                                .overlay(Capsule().strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                        }.padding(.bottom, 8)
                        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 12), count: typeSize.isAccessibilitySize ? 1 : 2), spacing: 12) {
                            ForEach(visibleCards) { card in
                                ZStack(alignment: .topTrailing) {
                                    NavigationLink {
                                        if let route = project.routeID {
                                            ConversationView(machine: machine, workspaceID: workspaceID, routeID: route, card: card, model: model)
                                        }
                                    } label: { AgentSnapshotCard(card: card) }
                                        .buttonStyle(.plain)
                                        .accessibilityLabel(card.title + "，" + card.profile + "，" + AgentSnapshotCard.status(card.state))
                                        .accessibilityValue(card.taskTitle ?? "")
                                        .accessibilityIdentifier("agent-card-\(card.id)")
                                        Button { options = card } label: {
                                            Image(systemName: "ellipsis").font(.caption).frame(width: 44, height: 44).contentShape(Rectangle())
                                        }.buttonStyle(WorkspacePressStyle()).accessibilityLabel(card.title + "，卡片操作")
                                            .accessibilityIdentifier("card-options-\(card.id)").foregroundStyle(WorkspaceStyle.muted).padding(4)
                                }
                            }
                        }.accessibilityIdentifier("project-agent-grid")
                    }
                } else if let project {
                    WorkspaceStatusView(title: project.state == "closed" ? "暂时无法加载项目" : "暂时无法读取卡片", icon: "folder", message: "请检查 \(machine.name) 是否在线，以及项目文件夹是否仍然可用。", inline: true) {}
                } else if let error {
                    WorkspaceStatusView(title: loading ? "正在重新连接" : "连接已中断", icon: "wifi.exclamationmark", message: error, inline: true) {
                        if loading { ProgressView() }
                        else { WorkspaceStatusButton(title: "重新连接") { retry += 1 } }
                    }
                }
                if let project, project.state == "open" {
                    Button(action: openCreation) {
                        Label("创建卡片", systemImage: "plus").font(.footnote.weight(.medium)).frame(maxWidth: .infinity, minHeight: 48)
                            .overlay(Capsule().strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                    }.buttonStyle(WorkspacePressStyle()).padding(.top, 14)
                        .accessibilityIdentifier("create-card-entry")
                }
            }.padding(.horizontal, 20).padding(.bottom, 26)
        }
    }

    var body: some View {
        projectContent.workspaceLoading(project == nil && error == nil, "正在读取卡片…")
        .background(.black)
        .navigationTitle(project?.displayName ?? name).navigationBarTitleDisplayMode(.inline)
        .modifier(WorkspaceChrome(model: model, manageComputers: manageComputers ?? {}, enabled: switchProject != nil))
        .refreshable { retry += 1 }
        .sheet(isPresented: $showingFilters) {
            NavigationStack {
                Form { TextField("搜索卡片", text: $search); cardFilters }
                    .navigationTitle("筛选卡片").navigationBarTitleDisplayMode(.inline)
                    .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { showingFilters = false } } }
            }
        }
        .sheet(isPresented: $showingCreation) {
            NewCardView(projectName: project?.displayName ?? name, unavailable: creationUnavailable,
                        busy: creatingProfile != nil, notice: creationNotice, pending: pendingCreation,
                        model: model, machine: machine, project: project,
                        agentDefinitions: agentDefinitions, revision: revision) { profile, title, definitionID in
                guard let project else { return }
                let expectedRevision = revision
                creationWork = Task {
                    await createCard(profile, title: title, definitionID: definitionID,
                                     in: project, revision: expectedRevision)
                }
            }
        }
        .sheet(item: $options, onDismiss: { showingConversation = conversation != nil }) { selection in
            if let project, let card = project.cards?.first(where: { $0.id == selection.id }) {
                CardOptionsView(model: model, machine: machine, project: project, card: card,
                                revision: revision, canControl: canCreate) {
                    guard let route = project.routeID else { return }
                    conversation = (card, route); options = nil
                }
            }
        }
        .navigationDestination(isPresented: $showingConversation) {
            if let conversation {
                ConversationView(machine: machine, workspaceID: workspaceID, routeID: conversation.route,
                                 card: conversation.card, model: model)
            }
        }
        .onChange(of: showingConversation) { _, showing in if !showing { conversation = nil } }
        .task(id: "\(scenePhase)-\(retry)-\(active)") {
            generation += 1
            let requestGeneration = generation
            project = nil; agentDefinitions = []; canCreate = false; error = nil
            guard scenePhase == .active && active else { loading = false; options = nil; return }
            loading = true
            do {
                for try await update in model.client.projectUpdates(machine, workspaceID: workspaceID) {
                    try Task.checkCancellation()
                    guard requestGeneration == generation else { return }
                    switch update {
                    case .current(let result):
                        let fresh = result.projects.first
                        if let creationRoute, fresh?.routeID != creationRoute { showingCreation = false }
                        if fresh?.routeID != project?.routeID || revision != result.grant.revision {
                            options = nil
                            if !showingConversation { conversation = nil }
                        }
                        if let options, fresh?.cards?.contains(where: { $0.id == options.id }) != true { self.options = nil }
                        project = fresh; error = nil; loading = false
                        agentDefinitions = result.agentDefinitions
                        revision = result.grant.revision
                        canCreate = result.grant.projects.contains { $0.workspaceID == workspaceID && $0.access == "control" }
                    case .recovering:
                        project = nil; agentDefinitions = []; canCreate = false; options = nil
                        error = "正在重新连接电脑…"; loading = true
                    }
                }
            } catch {
                guard requestGeneration == generation, !Task.isCancelled else { return }
                if model.selectedID == machine.id, let failure = error as? MobileError,
                   [.unauthorized, .wrongComputer, .invalidCredential].contains(failure) {
                    model.disconnect()
                }
                project = nil; agentDefinitions = []; loading = false; canCreate = false; options = nil
                self.error = error.localizedDescription
            }
        }
        .onChange(of: scenePhase) { _, phase in if phase != .active { creationWork?.cancel() } }
        .onDisappear { generation += 1; creationWork?.cancel() }
    }

    private var projectHeading: some View {
        HStack(spacing: 12) {
            Button { switchProject?() } label: {
                HStack(spacing: 8) {
                    Text(project?.displayName ?? name)
                        .font(.system(size: projectTitleSize, weight: .semibold))
                        .lineLimit(typeSize.isAccessibilitySize ? 2 : 1).truncationMode(.middle)
                    Image(systemName: "chevron.down")
                        .font(.caption.weight(.semibold)).foregroundStyle(WorkspaceStyle.muted)
                }.frame(minHeight: 44).contentShape(Rectangle())
            }.accessibilityIdentifier("project-switcher").accessibilityLabel(project?.displayName ?? name)
                .accessibilityValue(machine.name).accessibilityHint("切换项目").disabled(switchProject == nil)
            Spacer(minLength: 0)
            Button(action: openCreation) { WorkspaceCircleIcon(symbol: "plus") }
                .accessibilityLabel("创建卡片").accessibilityIdentifier("create-card-toolbar")
        }.buttonStyle(WorkspacePressStyle()).padding(.bottom, 12)
    }

    @ViewBuilder private var cardFilters: some View {
        Picker("卡片状态", selection: $stateFilter) {
            Text("全部状态").tag(String?.none)
            ForEach(Array(Set((project?.cards?.map(\.state) ?? []) + [stateFilter].compactMap { $0 })).sorted(), id: \.self) { state in
                Text(AgentSnapshotCard.status(state)).tag(Optional(state))
            }
        }
        Picker("Agent", selection: $profileFilter) {
            Text("全部 Agent").tag(String?.none)
            ForEach(Array(Set((project?.cards?.map(\.profile) ?? []) + [profileFilter].compactMap { $0 })).sorted(), id: \.self) { Text($0).tag(Optional($0)) }
        }
        if filtering { Button("清除筛选", action: clearFilters) }
    }

    @MainActor private func createCard(_ profile: String, title: String?, definitionID: Int64?,
                                       in expected: ProjectSnapshot, revision expectedRevision: UInt64) async {
        guard canCreate, creatingProfile == nil, pendingCreation == nil, scenePhase == .active,
              model.selectedID == machine.id, expected.state == "open",
              let route = expected.routeID, project?.routeID == route, revision == expectedRevision else { return }
        let context = generation
        creatingProfile = profile; creationNotice = nil
        defer { creatingProfile = nil }
        do {
            let command = try await model.client.prepareCardCreation(machine, workspaceID: workspaceID, routeID: route,
                revision: expectedRevision, profile: profile, title: title, definitionID: definitionID)
            try Task.checkCancellation()
            guard context == generation, canCreate, project?.routeID == route, revision == expectedRevision else { return }
            try await model.commandStore.save(command)
            try await model.reloadCommands()
            try Task.checkCancellation()
            var receipt = try await model.client.submit(command, to: machine)
            // Same bounded receipt query as CardCommandSheet: 10 × 500ms covers ordinary 360ms persistence.
            for _ in 0..<10 where receipt.state == "accepted" {
                try await Task.sleep(for: .milliseconds(500))
                receipt = try await model.client.receipt(for: command, on: machine)
            }
            try Task.checkCancellation()
            if receipt.state == "succeeded" {
                try await model.commandStore.remove(id: command.id)
                try await model.reloadCommands()
                clearFilters()
                creationNotice = "已创建 \(title ?? profile) 卡片"
                showingCreation = false
            } else {
                creationNotice = receipt.state == "rejected" ? "电脑未创建卡片" : "创建结果待确认"
            }
        } catch {
            if !Task.isCancelled { creationNotice = error.localizedDescription }
        }
    }

    private func openCreation() { creationNotice = nil; creationRoute = project?.routeID; showingCreation = true }

    private func clearFilters() { search = ""; stateFilter = nil; profileFilter = nil }
}

struct AgentSnapshotCard: View {
    let card: CardSnapshot
    @Environment(\.dynamicTypeSize) private var typeSize
    private var working: Bool { ["thinking", "running_tool"].contains(card.state) }
    static func status(_ state: String) -> String {
        switch state {
        case "idle": "未安排任务"
        case "preparing": "启动中"
        case "collecting": "整理成果"
        case "ready": "就绪"
        case "thinking": "思考中"
        case "running_tool": "执行中"
        case "interrupting": "正在停止"
        case "waiting_approval": "等待确认"
        case "done": "已完成"
        case "failed": "失败"
        case "ended": "已停止"
        default: "状态未确认"
        }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ProviderMark(profile: card.profile, size: 32, working: working).padding(.bottom, 24)
            Text(card.title)
                .font(.footnote.weight(.semibold)).lineLimit(typeSize.isAccessibilitySize ? nil : 2)
                .fixedSize(horizontal: false, vertical: true).frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityIdentifier("agent-card-name-\(card.id)")
            if let input = card.taskTitle, !input.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Text(input).font(.caption).foregroundStyle(WorkspaceStyle.muted)
                    .lineLimit(typeSize.isAccessibilitySize ? nil : 2)
                    .fixedSize(horizontal: false, vertical: true).frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.top, 6).accessibilityIdentifier("agent-card-input-\(card.id)")
            }
            Spacer(minLength: 20)
            HStack(spacing: 5) {
                Circle().frame(width: 5, height: 5)
                Text(working ? "进行中" : card.state == "idle" ? "空闲" : Self.status(card.state))
            }.font(.caption2).foregroundStyle(card.state == "waiting_approval" ? Color.orange : WorkspaceStyle.muted)
        }
        .padding(.horizontal, 17).padding(.top, 20).padding(.bottom, 16)
        .frame(maxWidth: .infinity, minHeight: 165, alignment: .topLeading)
        .background(WorkspaceStyle.surface, in: RoundedRectangle(cornerRadius: 23))
        .overlay {
            RoundedRectangle(cornerRadius: 23).stroke(working ? Color(white: 0.32) : WorkspaceStyle.line, lineWidth: 1)
            AgentActivityDecoration(kind: .border, active: working)
        }
        .accessibilityElement(children: .contain)
    }
}
