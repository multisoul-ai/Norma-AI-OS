import SwiftUI
import NormaCore

struct WorkspaceGroupChatView: View {
    @Bindable var model: WorkspaceChatModel
    let computers: ComputerModel
    let cloud: CloudChatModel
    let workspaceID: String
    @State private var settings = false
    @State private var creatingAgent = false
    @State private var openSettingsAfterCreating = false
    @State private var members = false
    @State private var overview = false
    @State private var detail: WorkspaceChatEvent?
    @State private var recording = false
    @State private var followingLatest = true
    @State private var scrolling = false
    @State private var newReplies = false
    @State private var scrollHeight: CGFloat = 0
    @State private var readingPosition = WorkspaceChatReadingPosition()
    @FocusState private var typing: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    private var workspace: DesktopChatWorkspace? { model.workspace(workspaceID) }
    private var available: Bool { model.isAvailable(workspaceID) }
    private var draft: WorkspaceChatDraft { model.drafts[workspaceID] ?? .init() }
    private var animation: Animation? { reduceMotion ? nil : .spring(response: 0.3, dampingFraction: 1) }
    private var text: Binding<String> { Binding(get: { draft.text }, set: { value in var copy = draft; copy.text = value; model.updateDraft(copy, workspaceID: workspaceID) }) }
    private var updates: [WorkspaceChatEvent] { model.events[workspaceID] ?? [] }
    private var entries: [WorkspaceOutboxEntry] { model.messages(workspaceID) }
    private var pendingEntries: [WorkspaceOutboxEntry] {
        entries.filter { entry in !updates.contains { $0.kind == "received" && $0.operationID == entry.id } }
    }
    var body: some View {
        VStack(spacing: 0) {
            summary.padding(.horizontal, 20).padding(.vertical, 12)
            ScrollViewReader { scroll in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 16) {
                        if let ideaID = draft.ideaID {
                            Label("来源想法 · \(ideaID.prefix(8))", systemImage: "link")
                                .font(.subheadline).padding(16).frame(maxWidth: .infinity, alignment: .leading)
                                .background(.white, in: RoundedRectangle(cornerRadius: 20))
                        }
                        if updates.isEmpty && entries.isEmpty { emptyState }
                        ForEach(workspaceChatTimeline(updates)) { event in
                            if event.kind == "received" {
                                userMessage(event.payload.text ?? "", entry: entries.first { $0.id == event.operationID }, timestamp: event.createdAtUnixMS)
                            } else { eventRow(event).background(WorkspaceChatScrollProbe(position: readingPosition, anchorID: event.id)) }
                        }
                        ForEach(pendingEntries) { entry in userMessage(entry.message.text, entry: entry) }
                        Color.clear.frame(height: 1).id("chat-bottom").background {
                            GeometryReader { geometry in Color.clear.preference(key: ChatBottomPosition.self, value: geometry.frame(in: .named("group-scroll")).maxY) }
                        }.background(WorkspaceChatScrollProbe(position: readingPosition))
                    }.padding(20)
                }.defaultScrollAnchor(.bottom)
                    .accessibilityIdentifier("group-message-scroll")
                    .coordinateSpace(name: "group-scroll")
                    .background { GeometryReader { geometry in Color.clear.preference(key: ChatViewportHeight.self, value: geometry.size.height) } }
                    .onPreferenceChange(ChatViewportHeight.self) { scrollHeight = $0 }
                    .onPreferenceChange(ChatBottomPosition.self) { bottom in
                        // Update follow mode only during direct user scrolling; appended content must not disable it.
                        if scrolling { followingLatest = bottom <= scrollHeight + 80 }
                    }
                    .simultaneousGesture(DragGesture(minimumDistance: 5).onChanged { _ in scrolling = true }.onEnded { _ in scrolling = false })
                    // Interactive keyboard resizing can repeatedly measure selectable messages on iOS 26.
                    .scrollDismissesKeyboard(.immediately).scrollIndicators(.hidden)
                    .background(IdeaPalette.background)
                    .overlay(alignment: .bottomTrailing) {
                        if newReplies {
                            Button { followingLatest = true; newReplies = false; withAnimation(animation) { scroll.scrollTo("chat-bottom", anchor: .bottom) } } label: {
                                Label("新回复", systemImage: "arrow.down").font(.subheadline.weight(.medium)).padding(12).background(.white, in: Capsule())
                            }.padding(16).accessibilityIdentifier("group-new-replies")
                        }
                    }
                    .safeAreaInset(edge: .bottom, spacing: 0) { if available { composer } else { Text("空间已从 Mac 移除或当前不可访问；仅供查看历史，原文与草稿仍保留。").font(.footnote).padding(16).background(.white) } }
                    .onChange(of: entries.count) { _, _ in followingLatest = true; newReplies = false; withAnimation(animation) { scroll.scrollTo("chat-bottom", anchor: .bottom) } }
                    .onChange(of: updates.last?.cursor) { _, _ in
                        if followingLatest && detail == nil { withAnimation(animation) { scroll.scrollTo("chat-bottom", anchor: .bottom) } }
                        else { newReplies = true }
                    }
                    .onChange(of: typing) { _, focused in if focused { withAnimation(animation) { scroll.scrollTo("chat-bottom", anchor: .bottom) } } }
            }
        }
        .background(IdeaPalette.background)
        .foregroundStyle(IdeaPalette.ink).tint(IdeaPalette.blue).buttonStyle(IdeaPressStyle())
        .navigationTitle(workspace?.name ?? "工作空间群聊").navigationBarTitleDisplayMode(.inline)
        .toolbar(.visible, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { readingPosition.preserve(); typing = false; creatingAgent = true } label: { Image(systemName: "plus").frame(width: 44, height: 44) }
                    .disabled(!available).accessibilityLabel("新建智能体").accessibilityIdentifier("group-create-agent")
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button { readingPosition.preserve(); typing = false; settings = true } label: { Image(systemName: "ellipsis").frame(width: 44, height: 44) }
                    .disabled(!available).accessibilityLabel("工作空间设置").accessibilityIdentifier("group-settings")
            }
        }
        .sheet(isPresented: $settings, onDismiss: { readingPosition.restore() }) { DesktopWorkspaceSettingsView(model: model, computers: computers, cloud: cloud, workspaceID: workspaceID) }
        .sheet(isPresented: $creatingAgent, onDismiss: { readingPosition.restore(); if openSettingsAfterCreating { openSettingsAfterCreating = false; settings = true } }) { AgentCreationSheet(model: model, workspaceID: workspaceID, addProject: { openSettingsAfterCreating = true }) }
        .sheet(isPresented: $members) { memberPicker }
        .sheet(isPresented: $overview) { overviewSheet }
        .sheet(item: $detail, onDismiss: { readingPosition.restore() }) { event in
            if event.payload.target != nil {
                let current = workspaceExecutionDetail(event, events: updates)
                WorkspaceExecutionDetailView(event: current, agentName: sender(event),
                    projectName: event.payload.target.flatMap { member($0)?.project.name } ?? "原项目",
                    computers: computers, cloud: cloud, canFollowUp: available) { target in
                        var copy = draft; copy.target = target; copy.target?.taskRef = current.payload.taskRef; copy.intent = .discuss
                        model.updateDraft(copy, workspaceID: workspaceID); detail = nil; typing = true
                    }
            } else { resultSheet(event) }
        }
        .task {
            model.activeWorkspaceID = workspaceID
            await model.synchronize(computers: computers, cloud: cloud, refresh: true)
        }
        .onDisappear { if model.activeWorkspaceID == workspaceID { model.activeWorkspaceID = nil } }
        .preferredColorScheme(.light)
    }
    private var summary: some View {
        Button { overview = true } label: {
            HStack(spacing: 14) {
                Image(systemName: "doc.text").font(.title2).foregroundStyle(.orange)
                    .frame(width: 44, height: 44).background(.orange.opacity(0.09), in: RoundedRectangle(cornerRadius: 14))
                VStack(alignment: .leading, spacing: 6) {
                    Text("工作空间概览").font(.headline)
                    Text("\(workspace?.projects.count ?? 0) 个项目 · \(workspace?.agents.count ?? 0) 位执行 Agent")
                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                    Label(model.online ? "Mac 已连接" : "离线 · 输入会保留", systemImage: model.online ? "circle.fill" : "wifi.slash")
                        .font(.caption).foregroundStyle(model.online ? IdeaPalette.positiveText : IdeaPalette.secondary)
                }
                Spacer(minLength: 4); Image(systemName: "chevron.right").foregroundStyle(IdeaPalette.secondary)
            }.padding(16).background(.white, in: RoundedRectangle(cornerRadius: 22))
        }.accessibilityIdentifier("group-overview")
    }
    private var emptyState: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack { Image("NormaPortrait").resizable().scaledToFill().frame(width: 38, height: 38).clipShape(Circle()); Text("Norma").font(.headline) }
            Text("在这里讨论想法，或选择「请执行」让我安排任务。也可以 @ 已加入的 Agent，继续它在 Mac 上的会话。")
                .font(.body).lineSpacing(4)
        }.padding(18).background(.white, in: RoundedRectangle(cornerRadius: 22))
    }
    private func userMessage(_ body: String, entry: WorkspaceOutboxEntry?, timestamp: Int64? = nil) -> some View {
        VStack(alignment: .trailing, spacing: 6) {
            if let target = entry?.message.target, let member = workspace?.agents.first(where: { $0.target.areaID == target.areaID && $0.target.nodeID == target.nodeID }) {
                Text("@\(member.agent.name) · \(member.project.name)").font(.caption).foregroundStyle(IdeaPalette.secondary)
            }
            Text(body).font(.body).lineSpacing(3).textSelection(.enabled)
                .padding(16).foregroundStyle(.white).background(IdeaPalette.blue, in: RoundedRectangle(cornerRadius: 22))
            if let entry {
                HStack(spacing: 8) {
                    if let timestamp, timestamp > 0 { Text(Date(timeIntervalSince1970: Double(timestamp) / 1000), style: .time) }
                    Text(model.messageErrors[entry.id] ?? status(entry.receipt?.state ?? "pending"))
                        .accessibilityIdentifier("group-delivery-\(entry.id)")
                }.font(.caption).foregroundStyle(IdeaPalette.secondary)
                if model.messageErrors[entry.id] != nil || entry.receipt?.state == "outcome_unknown" {
                    Button("核查原操作回执") { Task { await model.checkReceipt(entry, computers: computers, cloud: cloud) } }
                        .font(.subheadline).frame(minHeight: 44)
                }
            }
        }.frame(maxWidth: .infinity, alignment: .trailing).padding(.leading, 34)
    }
    @ViewBuilder private func eventRow(_ event: WorkspaceChatEvent) -> some View {
        if event.kind == "running" || event.kind == "submitted" || event.kind == "dispatch" {
            Button { readingPosition.preserve(anchorID: event.id); typing = false; detail = event } label: {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: "checkmark.circle").foregroundStyle(IdeaPalette.positiveText)
                    VStack(alignment: .leading, spacing: 4) {
                        HStack { Text("\(sender(event)) · \(event.kind == "running" ? "开始执行" : "已接收")").font(.subheadline.weight(.medium)); Spacer(); eventTime(event) }
                        Text(event.payload.text ?? "任务已接收").font(.caption).lineLimit(2).foregroundStyle(IdeaPalette.secondary)
                    }
                    Image(systemName: "chevron.right").font(.caption).foregroundStyle(IdeaPalette.secondary)
                }.padding(.vertical, 8).frame(minHeight: 44)
            }.accessibilityIdentifier("group-receipt-\(event.cursor)")
        } else if event.kind == "task_result" {
            Button { readingPosition.preserve(anchorID: event.id); typing = false; detail = event } label: {
                HStack(spacing: 12) {
                    Image(systemName: event.kind == "task_result" ? "doc.richtext" : "chevron.left.forwardslash.chevron.right")
                        .font(.title2).foregroundStyle(IdeaPalette.blue).frame(width: 44, height: 44)
                        .background(IdeaPalette.blue.opacity(0.08), in: RoundedRectangle(cornerRadius: 14))
                    VStack(alignment: .leading, spacing: 6) {
                        Text(event.kind == "task_result" ? "\(sender(event)) 的成果" : "\(sender(event)) 的任务").font(.headline)
                        HStack {
                            if let target = event.payload.target { Text(member(target)?.project.name ?? "原项目").font(.caption).foregroundStyle(IdeaPalette.secondary) }
                            Spacer(minLength: 4)
                            Text(status(event.payload.state ?? event.kind)).font(.caption.weight(.medium)).foregroundStyle(event.payload.state == "succeeded" ? IdeaPalette.positiveText : IdeaPalette.secondary)
                        }
                        eventTime(event)
                        Text((try? AttributedString(markdown: event.payload.text ?? "", options: .init(interpretedSyntax: .full))) ?? AttributedString(status(event.payload.state ?? "submitted")))
                            .font(.subheadline).lineLimit(3).foregroundStyle(IdeaPalette.secondary)
                    }
                    Spacer(minLength: 4); Image(systemName: "chevron.right")
                }.padding(16).background(.white, in: RoundedRectangle(cornerRadius: 22))
            }.accessibilityIdentifier("group-task-\(event.cursor)")
        } else {
            HStack(alignment: .top, spacing: 10) {
                if event.payload.sender == "Norma" { Image("NormaPortrait").resizable().scaledToFill().frame(width: 38, height: 38).clipShape(Circle()) }
                else { Image(systemName: "person.crop.circle").font(.title).frame(width: 38, height: 38) }
                VStack(alignment: .leading, spacing: 7) {
                    HStack { Text(sender(event)).font(.subheadline.weight(.medium)).foregroundStyle(IdeaPalette.secondary); Spacer(); eventTime(event) }
                    Text(event.payload.text ?? status(event.kind)).font(.body).lineSpacing(3).textSelection(.enabled)
                        .padding(16).frame(maxWidth: .infinity, alignment: .leading).background(.white, in: RoundedRectangle(cornerRadius: 22))
                    if let error = event.payload.error { Text(WorkspaceChatError.server(error).localizedDescription).font(.caption).foregroundStyle(.red) }
                }
            }.accessibilityIdentifier("group-response-\(event.cursor)")
        }
    }
    private var composer: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let target = draft.target {
                HStack {
                    Label(targetLabel(target), systemImage: "at")
                        .font(.caption).lineLimit(2)
                    Spacer(minLength: 4)
                    Button { var copy = draft; copy.target = nil; model.updateDraft(copy, workspaceID: workspaceID) } label: { Image(systemName: "xmark.circle.fill").frame(width: 44, height: 44) }.accessibilityLabel("取消指定 Agent")
                }.transition(.opacity)
            }
            HStack(alignment: .bottom, spacing: 6) {
                Menu {
                    Button("讨论，不派工", systemImage: "bubble.left.and.bubble.right") { setIntent(.discuss) }
                    Button("请执行，允许派工", systemImage: "play") { setIntent(.execute) }
                } label: { Image(systemName: "plus").font(.title2).frame(width: 44, height: 44) }.accessibilityLabel("消息模式：\(draft.intent == .discuss ? "讨论" : "请执行")").accessibilityIdentifier("group-message-mode")
                ChatVoiceInput(text: text, recording: $recording, disabled: false, focus: { typing = $0 }) {
                TextField(draft.intent == .discuss ? "发消息或 @Agent" : "描述要执行的任务", text: text, axis: .vertical)
                    .font(.body).lineLimit(1...5).focused($typing).padding(12)
                    .background(IdeaPalette.softGray, in: RoundedRectangle(cornerRadius: 24)).accessibilityIdentifier("group-message-input")
                }
                Button { members = true } label: { Image(systemName: "at").font(.title2).frame(width: 44, height: 44) }
                    .accessibilityLabel("选择 Agent").accessibilityIdentifier("group-mention")
                Button {
                    Task {
                        if await model.send(workspaceID: workspaceID, intent: draft.intent, target: draft.target, ideaID: draft.ideaID) {
                            await model.synchronize(computers: computers, cloud: cloud)
                        }
                    }
                } label: { Image(systemName: "arrow.up").font(.headline).frame(width: 44, height: 44).foregroundStyle(.white).background(IdeaPalette.blue, in: Circle()) }
                    .disabled(recording || draft.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .accessibilityLabel(draft.intent == .discuss ? "发送消息" : "派发任务").accessibilityIdentifier("group-send")
            }
            HStack {
                Text(draft.intent == .discuss ? "讨论 · Norma 回应" : "请执行 · Norma 可派工").font(.caption).foregroundStyle(IdeaPalette.secondary)
                Spacer()
                if !model.online { Text("保存为待发送").font(.caption).foregroundStyle(IdeaPalette.secondary) }
            }.padding(.horizontal, 8)
            if let error = model.error { Text(error).font(.caption).foregroundStyle(.red).padding(.horizontal, 8) }
        }.padding(10).background {
            let shape = RoundedRectangle(cornerRadius: 28)
            if reduceTransparency { shape.fill(.white) }
            else if #available(iOS 26, *) { Color.clear.glassEffect(.regular, in: shape) }
            else { shape.fill(.regularMaterial) }
        }.padding(.horizontal, 12).padding(.vertical, 8).animation(animation, value: draft.target)
    }
    private func setIntent(_ intent: WorkspaceMessageIntent) { var copy = draft; copy.intent = intent; model.updateDraft(copy, workspaceID: workspaceID) }
    private var memberPicker: some View {
        NavigationStack {
            List {
                Button { var copy = draft; copy.target = nil; model.updateDraft(copy, workspaceID: workspaceID); members = false } label: {
                    HStack { Label("Norma · 协调工作空间", systemImage: "sparkles"); Spacer(); if draft.target == nil { Image(systemName: "checkmark") } }
                }.accessibilityAddTraits(draft.target == nil ? [.isSelected] : [])
                ForEach(workspace?.agents ?? []) { member in
                    let selected = draft.target?.areaID == member.target.areaID && draft.target?.nodeID == member.target.nodeID && draft.target?.definitionID == member.target.definitionID
                    Button {
                        var copy = draft; copy.target = member.target; model.updateDraft(copy, workspaceID: workspaceID); members = false
                    } label: { HStack { VStack(alignment: .leading, spacing: 6) { Text(member.agent.name).font(.headline); Text("\(member.project.name) · \(status(member.agent.card.state))").font(.subheadline).foregroundStyle(IdeaPalette.secondary) }; Spacer(); if selected { Image(systemName: "checkmark") } } }
                        .disabled(["preparing", "failed", "ended", "disabled"].contains(member.agent.card.state))
                        .accessibilityIdentifier("group-member-\(member.id)")
                        .accessibilityAddTraits(selected ? [.isSelected] : [])
                }
                if workspace?.agents.isEmpty != false { Text("在工作空间设置中添加 Mac 的 Agent 卡片。") }
            }.navigationTitle("选择 Agent").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { members = false } } }
        }.presentationDetents([.medium, .large]).presentationDragIndicator(.visible)
    }
    private var overviewSheet: some View {
        NavigationStack {
            List {
                Section("关联项目") { ForEach(workspace?.projects ?? []) { project in VStack(alignment: .leading, spacing: 6) { Text(project.name).font(.headline); Text(project.root).font(.footnote).foregroundStyle(IdeaPalette.secondary) } } }
                Section("参与 Agent") { Text("Norma · Chief"); ForEach(workspace?.agents ?? []) { member in Text("\(member.agent.name) · \(member.project.name)") } }
                Section("共享上下文") { Text("本工作空间的消息、配置、任务回执与成果。项目文件和工作树记忆由 Mac 按实际目标读取。") }
            }.navigationTitle("工作空间概览").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { overview = false } } }
        }.presentationDragIndicator(.visible)
    }
    private func resultSheet(_ event: WorkspaceChatEvent) -> some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    Text("\(sender(event)) · \(status(event.payload.state ?? event.kind))").font(.title3.bold())
                    eventTime(event)
                    WorkspaceResultTextView(text: event.payload.text ?? "Mac 尚未提供正文结果。")

                    if let target = event.payload.target {
                        Text(member(target)?.project.name ?? "原项目").font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                        DisclosureGroup("执行详情") {
                            Text("任务：\(event.payload.taskRef ?? "无")\n项目区域：\(target.areaID)\n卡片：\(target.nodeID)\n运行：\(target.runID ?? "尚未绑定")\n会话：\(target.sessionID ?? "尚未绑定")").font(.footnote.monospaced()).foregroundStyle(IdeaPalette.secondary).textSelection(.enabled)
                        }
                        Button("追问这个任务") {
                            var copy = draft; copy.target = target; copy.target?.taskRef = event.payload.taskRef; copy.intent = .discuss
                            model.updateDraft(copy, workspaceID: workspaceID); detail = nil; typing = true
                        }.frame(minHeight: 48).disabled(!available || target.runID == nil || target.sessionID == nil).accessibilityIdentifier("group-followup")
                    }
                    if let error = event.payload.error { Text(WorkspaceChatError.server(error).localizedDescription).foregroundStyle(.red) }
                }.padding(20).frame(maxWidth: .infinity, alignment: .leading)
            }.background(IdeaPalette.background).navigationTitle("成果与回执").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { detail = nil } } }
        }.presentationDragIndicator(.visible)
    }
    private func member(_ target: WorkspaceAgentTarget) -> DesktopChatMember? {
        workspace?.agents.first { $0.target.areaID == target.areaID && $0.target.nodeID == target.nodeID && $0.target.definitionID == target.definitionID }
    }
    private func targetLabel(_ target: WorkspaceAgentTarget) -> String {
        let destination = member(target)
        return "\(target.taskRef == nil ? "" : "追问 ")\(destination?.agent.name ?? "原 Agent") · \(destination?.project.name ?? "原项目")"
    }
    private func sender(_ event: WorkspaceChatEvent) -> String {
        if let target = event.payload.target { return member(target)?.agent.name ?? event.payload.sender ?? "原 Agent" }
        return event.payload.sender ?? "Mac"
    }
    @ViewBuilder private func eventTime(_ event: WorkspaceChatEvent) -> some View {
        if let milliseconds = event.createdAtUnixMS, milliseconds > 0 {
            Text(Date(timeIntervalSince1970: Double(milliseconds) / 1000), style: .time).font(.caption).foregroundStyle(IdeaPalette.secondary)
        }
    }
    private func status(_ value: String) -> String { workspaceChatStatus(value) }
}

private struct ChatBottomPosition: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}
private struct ChatViewportHeight: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}
