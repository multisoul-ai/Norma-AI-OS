import SwiftUI
import NormaCore

struct ConversationView: View {
    let machine: PairedMachine
    let workspaceID: String
    let routeID: String
    let card: CardSnapshot
    let model: ComputerModel
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var showTerminal = false
    @State private var timeline = ConversationTimeline()
    @State private var loading = false
    @State private var error: String?
    @State private var notice: String?
    @State private var checkedAt: Date?
    @State private var retry = 0
    @State private var paging: Task<Void, Never>?
    @State private var currentCard: CardSnapshot?
    @State private var project: ProjectSnapshot?
    @State private var revision: UInt64 = 0
    @State private var canControl = false
    @State private var binding: ConversationRequest?
    @State private var watchEnabled = false
    @State private var connectionError: String?
    @State private var historyEpoch = 0
    @State private var followsLatest = true
    @State private var scrollRequest = 0

    @State private var stopping = false
    @State private var showStopResult = false
    private var pendingStop: PendingMobileCommand? {
        model.pendingCommands.first {
            guard $0.machineID == machine.id, $0.request.workspaceID == workspaceID,
                  $0.request.routeID == routeID, case let .interruptTurn(expected) = $0.request.command else { return false }
            return expected.nodeID == card.id
        }
    }
    private var active: CardSnapshot { currentCard ?? card }
    private var canStop: Bool {
        canControl && scenePhase == .active && active.mobileControl && active.canInterrupt != false && active.runID != nil
        && active.sessionID != nil && active.turnID != nil
        && ["thinking", "running_tool", "waiting_approval"].contains(active.state)
    }

    private var resultsMenu: some View {
                Menu {
                    NavigationLink {
                        TaskDeliveriesView(model: model, machine: machine, workspaceID: workspaceID, routeID: routeID, card: active)
                    } label: { Label("任务成果", systemImage: "tray.full") }
                    NavigationLink {
                        ProjectReviewView(machine: machine, workspaceID: workspaceID, routeID: routeID, model: model)
                    } label: { Label("项目当前变更", systemImage: "doc.text.magnifyingglass") }
                } label: { Image(systemName: "doc.text.magnifyingglass") }
                .disabled(project == nil).accessibilityLabel("查看成果与变更").accessibilityIdentifier("chat-deliveries-menu")
    }
    private var chatContent: some View {
        ScrollViewReader { reader in
        GeometryReader { viewport in
        ScrollView {
            // Lazy height estimates can loop when a selectable reply is taller than the viewport.
            // ponytail: lay out loaded history eagerly; use message virtualization if history size becomes costly.
            VStack(alignment: .leading, spacing: 20) {
                (typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading, spacing: 4)) : AnyLayout(HStackLayout())) {
                    Label(machine.name, systemImage: "laptopcomputer")
                        .lineLimit(typeSize.isAccessibilitySize ? nil : 1)
                        .fixedSize(horizontal: false, vertical: true)
                    if !typeSize.isAccessibilitySize { Spacer() }
                    if let checkedAt { Text(checkedAt, style: .time) }
                }.font(.caption).foregroundStyle(.secondary)
                if let connectionError { Text(connectionError).font(.footnote).foregroundStyle(.orange) }
                if let error { Text(error).font(.footnote).foregroundStyle(.orange) }
                if let notice { Text(notice).font(.footnote).foregroundStyle(.secondary) }
                if MobileAgentProfile.historyPaginatesBackward(card.profile), timeline.hasMore {
                    Button("加载更早的消息") { paging = Task { await loadOlder() } }
                        .font(.subheadline).frame(minHeight: 44).disabled(loading || paging != nil)
                }
                if binding != nil || project == nil, timeline.items.isEmpty && error == nil && !loading {
                    Text("等待 Agent 写入对话…").font(.subheadline).foregroundStyle(.secondary)
                }
                ForEach(timeline.items) { item in
                    ConversationMessageRow(item: item, profile: card.profile, imageSource: {
                        try await model.client.chatImage($0, on: machine, workspaceID: workspaceID)
                    })
                }
                if !MobileAgentProfile.historyPaginatesBackward(card.profile), timeline.hasMore {
                    WorkspaceLoadingView(message: "正在读取后续消息…", inline: true)
                }
                if watchEnabled, active.isBusy, let run = active.runID {
                    LiveAgentOutput(client: model.client, machine: machine,
                        request: TerminalRequest(workspaceID: workspaceID, routeID: routeID, nodeID: card.id,
                                                 runID: run, sessionID: active.sessionID), profile: card.profile) {
                        if followsLatest { scrollRequest += 1 }
                    }
                }
                Color.clear.frame(height: 1).id("conversation-bottom")
            }.padding(16)
            .background {
                GeometryReader { content in
                    Color.clear.preference(key: ConversationBottom.self,
                                           value: content.frame(in: .named("chat-scroll")).maxY)
                }
            }
        }.accessibilityIdentifier("chat-message-list")
        .overlay {
            if binding == nil, project != nil {
                ContentUnavailableView("开始对话", systemImage: "bubble.left.and.bubble.right", description: Text("告诉 Agent 你想完成什么。"))
                    .allowsHitTesting(false)
            }
        }
        .workspaceLoading(loading && timeline.items.isEmpty, "正在读取对话…")
        .coordinateSpace(name: "chat-scroll")
        .modifier(ChatScrollTracking(followsLatest: $followsLatest, viewportHeight: viewport.size.height))
        .onChange(of: timeline.items.last) { _, _ in
            if followsLatest { reader.scrollTo("conversation-bottom", anchor: .bottom) }
        }
        .onChange(of: viewport.size.height) { _, _ in
            if followsLatest { reader.scrollTo("conversation-bottom", anchor: .bottom) }
        }
        .onChange(of: active.isBusy) { _, _ in
            if followsLatest { reader.scrollTo("conversation-bottom", anchor: .bottom) }
        }
        .onChange(of: scrollRequest) { _, _ in
            followsLatest = true; reader.scrollTo("conversation-bottom", anchor: .bottom)
        }
        .overlay(alignment: .bottomTrailing) {
            if !followsLatest {
                Button("最新消息", systemImage: "arrow.down") { scrollRequest += 1 }
                    .font(.caption).padding(.horizontal, 14).frame(minHeight: 44)
                    .background(.regularMaterial, in: Capsule()).padding(12)
                    .accessibilityIdentifier("chat-latest-message")
            }
        }
        }
        }
    }
    var body: some View {
        VStack(spacing: 0) {
            Picker("任务内容", selection: $showTerminal) {
                Text("对话").tag(false)
                Text("终端").tag(true)
            }.pickerStyle(.segmented).padding(.horizontal, 16).padding(.vertical, 8)
                .accessibilityIdentifier("chat-content-tabs")
            if showTerminal {
                TerminalPanel(model: model, machine: machine, workspaceID: workspaceID, routeID: routeID,
                              card: active, project: project, revision: revision, canControl: canControl)
            } else {
                // Keep history and the keyboard-aware composer in separate layout regions.
                chatContent.clipped()
                ChatComposer(model: model, machine: machine, workspaceID: workspaceID, routeID: routeID, project: project, card: currentCard ?? card, revision: revision, canControl: canControl && scenePhase == .active) { result in
                    followsLatest = true; scrollRequest += 1
                    guard let run = result.runID, let session = result.sessionID else { return }
                    if let binding, binding.runID != run || binding.sessionID != session { notice = "已确认旧会话的发送结果，请重新打开当前卡片。"; return }
                    if binding == nil { timeline = ConversationTimeline() }
                    binding = ConversationRequest(workspaceID: workspaceID, routeID: routeID, nodeID: card.id, runID: run, sessionID: session, cursor: nil)
                    error = nil
                }
            }
        }
        .background(.black)
        .scrollDismissesKeyboard(.interactively)
        .navigationTitle(typeSize.isAccessibilitySize ? "" : card.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.visible, for: .navigationBar)
        .toolbar(.hidden, for: .tabBar)
        .toolbar {
            if typeSize.isAccessibilitySize {
                ToolbarItem(placement: .principal) {
                    // Navigation stays compact; message and tool content retain the full accessibility size.
                    Text(card.title).font(.headline).lineLimit(1).minimumScaleFactor(0.7)
                        .dynamicTypeSize(...DynamicTypeSize.large)
                        .accessibilityLabel(card.title).accessibilityIdentifier("chat-full-title")
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                resultsMenu
            }
            ToolbarItem(placement: .topBarTrailing) {
                if active.showsInterruptAction || (active.supportsInterruptAction && pendingStop != nil) {
                    Button {
                        if pendingStop != nil { showStopResult = true }
                        else { Task { await stop() } }
                    } label: {
                        if stopping || active.state == "interrupting" { ProgressView().frame(width: 24, height: 24) }
                        else { Image(systemName: pendingStop == nil ? "stop.circle" : "clock") }
                    }
                    .frame(minWidth: 44, minHeight: 44)
                    .disabled(stopping || project == nil || (pendingStop == nil && !canStop))
                    .accessibilityLabel(pendingStop != nil ? "查看停止结果" : active.state == "interrupting" ? "正在停止任务" : "停止任务")
                    .accessibilityIdentifier("chat-stop-task")
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button { timeline = ConversationTimeline(); error = nil; retry += 1 } label: { Image(systemName: "arrow.clockwise") }
                    .accessibilityLabel("重新读取对话")
            }
        }
        .task(id: "\(scenePhase)-\(retry)") {
            clearConnection(); connectionError = nil
            guard scenePhase == .active else { return }
            // Start the read-only history alongside project discovery. The history endpoint
            // validates this exact run/session itself; sending still waits for accept(snapshot).
            if binding == nil, let run = card.runID, let session = card.sessionID {
                binding = ConversationRequest(workspaceID: workspaceID, routeID: routeID,
                    nodeID: card.id, runID: run, sessionID: session, cursor: nil)
            }
            watchEnabled = binding != nil
            do {
                for try await update in model.client.projectUpdates(machine, workspaceID: workspaceID) {
                    try Task.checkCancellation()
                    switch update {
                    case .current(let snapshot): try accept(snapshot)
                    case .recovering:
                        clearConnection(preservingHistory: true); connectionError = "正在重新连接电脑…"
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                let invalidated = (error as? MobileError).map {
                    [.unauthorized, .wrongComputer, .invalidCredential, .sessionChanged].contains($0)
                } ?? false
                clearConnection(preservingHistory: !invalidated); connectionError = error.localizedDescription
                if model.selectedID == machine.id, let failure = error as? MobileError,
                   [.unauthorized, .wrongComputer, .invalidCredential].contains(failure) { model.disconnect() }
            }
        }
        .task(id: "\(scenePhase)-\(showTerminal)-\(watchEnabled)-\(binding?.runID ?? "none")-\(retry)") {
            historyEpoch += 1; paging?.cancel(); paging = nil
            timeline = ConversationTimeline(); error = nil
            guard scenePhase == .active, !showTerminal, watchEnabled, let binding else { loading = false; return }
            loading = true
            do {
                for try await update in model.client.conversationUpdates(machine, request: binding) {
                    try Task.checkCancellation()
                    switch update {
                    case .current(let page):
                        // Replace the last verified view only when the fresh stream has a valid page.
                        if loading { timeline = ConversationTimeline() }
                        if timeline.merge(page, continuing: false) { notice = "已重新同步最近对话，可加载更早的消息。" }
                        else if loading { notice = nil }
                        loading = false; error = nil
                    case .heartbeat: break
                    case .recovering:
                        historyEpoch += 1; paging?.cancel(); paging = nil
                        loading = true; notice = "正在重新连接对话…"
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                loading = false; notice = nil; timeline = ConversationTimeline(); self.error = error.localizedDescription
                if (error as? MobileError) == .sessionChanged { clearConnection() }
            }
        }
        .sheet(isPresented: $showStopResult) {
            if let project, let pendingStop {
                CardCommandSheet(model: model, machine: machine, project: project, revision: revision, pending: pendingStop)
            }
        }
        .onDisappear { paging?.cancel() }
    }

    @MainActor private func stop() async {
        guard canStop, !stopping, pendingStop == nil else { return }
        stopping = true
        defer { stopping = false }
        do {
            let command = try await model.client.prepareInterrupt(machine, workspaceID: workspaceID, routeID: routeID, revision: revision, card: active)
            try await model.commandStore.save(command)
            try await model.reloadCommands()
            _ = try await model.client.submit(command, to: machine)
            showStopResult = true
        } catch { self.error = error.localizedDescription; showStopResult = pendingStop != nil }
    }

    @MainActor private func clearConnection(preservingHistory: Bool = false) {
        // Keep the bound card identity for local drafts and keyboard focus. Only a fresh
        // verified snapshot can restore actions; reconnecting must not select the old run.
        project = nil; canControl = false; checkedAt = nil
        // History has its own pinned connection and run/session/grant checks.
        if !preservingHistory { watchEnabled = false }
        showStopResult = false; paging?.cancel(); paging = nil
    }

    @MainActor private func accept(_ snapshot: MachineSnapshot) throws {
        guard let project = snapshot.projects.first, project.routeID == routeID,
              let current = project.cards?.first(where: { $0.id == card.id }) else { throw MobileError.sessionChanged }
        if let binding {
            guard current.runID == binding.runID, current.sessionID == binding.sessionID else { throw MobileError.sessionChanged }
        } else if let originalRun = card.runID {
            guard current.runID == originalRun, card.sessionID == nil || current.sessionID == card.sessionID else { throw MobileError.sessionChanged }
        }
        self.project = project; currentCard = current; revision = snapshot.grant.revision
        canControl = snapshot.grant.projects.contains { $0.workspaceID == workspaceID && $0.access == "control" }
        checkedAt = .now; connectionError = nil
        if let run = current.runID, let session = current.sessionID {
            binding = ConversationRequest(workspaceID: workspaceID, routeID: routeID, nodeID: card.id, runID: run, sessionID: session, cursor: nil)
            watchEnabled = true
        }
    }

    @MainActor private func loadOlder() async {
        guard var request = binding, let cursor = timeline.cursor else { paging = nil; return }
        let epoch = historyEpoch
        defer { if epoch == historyEpoch { paging = nil } }
        request.cursor = cursor
        do {
            let response = try await model.client.conversation(machine, request: request)
            try Task.checkCancellation()
            guard epoch == historyEpoch else { return }
            timeline.merge(response.page, continuing: true)
        } catch {
            guard !Task.isCancelled, epoch == historyEpoch else { return }
            self.error = error.localizedDescription
        }
    }
}

private struct ConversationBottom: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}

private struct ChatScrollTracking: ViewModifier {
    @Binding var followsLatest: Bool
    let viewportHeight: CGFloat
    @State private var bottomVisible = true
    @State private var dragging = false

    func body(content: Content) -> some View {
        if #available(iOS 18.0, *) {
            content
                .onScrollGeometryChange(for: Bool.self) { geometry in
                    // Include the composer/safe-area inset; 24pt covers padding and rounding.
                    geometry.contentSize.height - geometry.contentOffset.y
                        <= geometry.containerSize.height - geometry.contentInsets.bottom + 24
                } action: { _, visible in
                    bottomVisible = visible
                    if visible && !dragging { followsLatest = true }
                }
                .onScrollPhaseChange { _, phase in
                    if phase == .interacting || phase == .decelerating {
                        dragging = true; followsLatest = false
                    } else if phase == .idle {
                        dragging = false; followsLatest = bottomVisible
                    }
                }
        } else {
            content.onPreferenceChange(ConversationBottom.self) { bottom in
                bottomVisible = bottom <= viewportHeight + 24
                if bottomVisible && !dragging { followsLatest = true }
            }
            .simultaneousGesture(DragGesture().onChanged { value in
                dragging = true
                if value.translation.height > 0 { followsLatest = false }
            }.onEnded { _ in dragging = false })
        }
    }
}
