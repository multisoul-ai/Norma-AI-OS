import SwiftUI
import NormaCore

struct WorkThreadView: View {
    @Bindable var library: WorkspaceLibraryModel
    let thread: WorkThread
    @Environment(IdeaLibraryModel.self) private var ideas
    @Environment(ComputerModel.self) private var computers
    @Environment(CloudChatModel.self) private var cloud
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @State private var messages: [WorkThreadMessage] = []
    @State private var tasks: [ThreadTaskOutboxOperation] = []
    @State private var followups: [ThreadFollowupOutboxOperation] = []
    @State private var selectedReviewID: String?
    @State private var focusOnReturn = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var selectedTaskID: String?
    @State private var showingMentions = false
    @State private var showingSettings = false
    @State private var openSettingsAfterMention = false
    @State private var resumeComposing = false
    @State private var loaded = false
    @State private var saving = false
    @State private var loadError: String?
    @State private var error: String?
    @State private var showingBrief = false
    @State private var showingSource = false
    @State private var originalIdea: Idea?
    @State private var scrollRequest = 0
    @State private var messageRead = UUID()
    @FocusState private var composing: Bool
    private var workspace: CollaborationWorkspace? { library.workspaces.first { $0.id == thread.workspaceID } }
    private var followupTaskID: String? { library.messageDrafts[thread.id]?.followupTaskID }
    private var selectedAgents: [WorkspaceAgentReference] { library.messageDrafts[thread.id]?.mentions ?? [] }
    private var draft: Binding<String> {
        Binding(get: { library.messageDrafts[thread.id]?.text ?? "" }, set: { library.updateMessageDraft($0, threadID: thread.id) })
    }

    var body: some View {
        ScrollViewReader { scroll in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    Button { showingBrief = true } label: {
                        HStack(spacing: 14) {
                            Image(systemName: "doc.text").font(.system(size: 22)).foregroundStyle(.orange)
                                .frame(width: 44, height: 44).background(.yellow.opacity(0.12), in: RoundedRectangle(cornerRadius: 14))
                            VStack(alignment: .leading, spacing: 5) {
                                Text("事项目标与决定").font(.headline)
                                Text("目标 · 关键决定 · 共同事实").font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                            }
                            Spacer(minLength: 0)
                            Image(systemName: "chevron.right").foregroundStyle(IdeaPalette.secondary)
                        }.threadCard()
                    }.accessibilityIdentifier("thread-work-brief")
                    if thread.sourceIdeaID != nil {
                        Button { showingSource = true } label: {
                            VStack(alignment: .leading, spacing: 10) {
                                HStack(spacing: 12) {
                                    Image(systemName: "link").font(.system(size: 20)).foregroundStyle(IdeaPalette.green)
                                        .frame(width: 36, height: 36).background(IdeaPalette.mint.opacity(0.22), in: Circle())
                                    Text("来源想法：\(thread.sourceExcerpt ?? "")")
                                        .font(.subheadline.weight(.medium)).lineLimit(2)
                                    Spacer(minLength: 0)
                                    Image(systemName: "chevron.right").font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                                }
                                Text("原始想法仍保存在想法库，可随时回看。")
                                    .font(.footnote).foregroundStyle(IdeaPalette.secondary)
                            }.threadCard()
                        }.accessibilityIdentifier("thread-source-idea")
                    }
                    if !loaded { ProgressView("正在读取对话…") }
                    else if messages.isEmpty {
                        Text("在这里讨论这件事，所有记录会长期保留。")
                            .font(.subheadline).foregroundStyle(IdeaPalette.secondary).padding(.vertical, 20)
                    }
                    ForEach(Array(messages.enumerated()), id: \.element.id) { index, message in
                        // One measured row per message keeps LazyVStack stable as receipts/cards appear.
                        VStack(alignment: .leading, spacing: 18) {
                        if index == 0 || !Calendar.current.isDate(messages[index - 1].createdAt, inSameDayAs: message.createdAt) {
                            Text(message.createdAt, format: .dateTime.year().month().day())
                                .font(.caption).foregroundStyle(IdeaPalette.secondary).frame(maxWidth: .infinity)
                        }
                        if message.author == "agent" || message.author == "norma" {
                            agentReply(message)
                        } else {
                        VStack(alignment: .trailing, spacing: 7) {
                            Text(message.text).font(.body).textSelection(.enabled)
                                .padding(14).foregroundStyle(.white)
                                .background(IdeaPalette.blue, in: RoundedRectangle(cornerRadius: 20))
                                .contextMenu {
                                    Button("记为共同决定", systemImage: "doc.text") {
                                        Task { await recordDecision(message) }
                                    }.disabled(workspace?.archived != false || saving)
                                }
                            HStack(spacing: 8) {
                                Text(message.createdAt, style: .time)
                                Text(message.state == .delivered ? "已送达 Mac" : "待发送")
                                Image(systemName: message.state == .delivered ? "checkmark.circle" : "clock")
                            }.font(.caption).foregroundStyle(IdeaPalette.secondary)
                            if message.state == .pending, let failure = library.deliveryErrors[message.id] {
                                Text(failure).font(.footnote).foregroundStyle(IdeaPalette.secondary)
                            }
                        }.padding(.leading, 40).frame(maxWidth: .infinity, alignment: .trailing)
                            .accessibilityElement(children: .ignore)
                            .accessibilityLabel("你，\(message.createdAt.formatted(date: .abbreviated, time: .shortened))，\(message.text)，\(message.state == .delivered ? "已送达 Mac" : "待发送")")
                            .accessibilityValue(message.state == .pending ? (library.deliveryErrors[message.id] ?? "") : "")
                            .accessibilityIdentifier("thread-message-\(message.id)")
                            .accessibilityActions {
                                if workspace?.archived == false && !saving {
                                    Button("记为共同决定") { Task { await recordDecision(message) } }
                                }
                            }
                        }
                        if let followup = followups.first(where: { $0.followup.sourceMessageID == message.id }) {
                            WorkThreadFollowupStatus(operation: followup,
                                agent: tasks.first(where: { $0.id == followup.followup.parentOperationID })?.target?.name ?? "Agent",
                                deliveryError: library.taskDeliveryErrors[followup.id]) {
                                    selectedTaskID = followup.followup.parentOperationID
                                }
                        }
                        let cards = message.timelineCards(tasks: tasks, history: messages)
                        ForEach(cards.tasks) { task in
                            Button { selectedTaskID = task.id } label: {
                                WorkThreadTaskCard(operation: task, workspace: workspace,
                                                   deliveryError: library.taskDeliveryErrors[task.id])
                            }.accessibilityIdentifier("thread-task-\(task.id)")
                        }
                        ForEach(cards.deliveries) { task in
                            if task.receipt?.execution?.execution.delivery?.available == true {
                                WorkThreadDeliveryCard(operation: task, workspace: workspace) { selectedReviewID = task.id }
                            }
                        }
                        }.id(message.id)
                    }
                    if !messages.isEmpty {
                        Text(messages.contains(where: { $0.state == .pending })
                             ? "消息已保存在这部 iPhone，满足发送条件后会继续发送。"
                             : tasks.isEmpty ? "消息已送达 Mac。" : "对话与任务记录已保存在这部 iPhone。")
                            .font(.footnote).foregroundStyle(IdeaPalette.secondary)
                            .frame(maxWidth: .infinity).multilineTextAlignment(.center)
                    }
                    if let failure = library.deliveryError {
                        Text(failure).font(.footnote).foregroundStyle(.red)
                    }
                    if let loadError {
                        Text(loadError).font(.subheadline).foregroundStyle(.red)
                        if !loaded { Button("重新读取") { Task { await load() } } }
                    }
                    Color.clear.frame(height: 1).id("thread-end")
                }.padding(20)
            }.scrollDismissesKeyboard(.interactively)
                .onChange(of: scrollRequest) { _, _ in scroll.scrollTo("thread-end", anchor: .bottom) }
                .onChange(of: loaded) { _, ready in if ready { scroll.scrollTo("thread-end", anchor: .bottom) } }
        }
        .background(IdeaPalette.background).foregroundStyle(IdeaPalette.ink).buttonStyle(IdeaPressStyle())
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                VStack(spacing: 3) {
                    Text(thread.title).font(.headline).lineLimit(1)
                    Text("\(workspace?.name ?? "工作空间") · \(workspace?.archived == true ? "已归档" : "讨论中")")
                        .font(.caption).foregroundStyle(IdeaPalette.secondary)
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button { showingBrief = true } label: { Image(systemName: "ellipsis") }
                    .accessibilityLabel("查看工作简报")
            }
        }
        .safeAreaInset(edge: .bottom) { composer }
        .navigationDestination(isPresented: $showingBrief) { WorkBriefView(library: library, thread: thread) }
        .sheet(isPresented: $showingSource) { sourceSheet }
        .sheet(isPresented: $showingMentions, onDismiss: {
            if openSettingsAfterMention { openSettingsAfterMention = false; showingSettings = true }
            else { composing = resumeComposing }
        }) { mentionSheet }
        .fullScreenCover(isPresented: $showingSettings) {
            if let workspace { WorkspaceSettingsView(library: library, computers: computers, workspace: workspace) }
        }
        .navigationDestination(item: $selectedTaskID) { id in
            if let task = tasks.first(where: { $0.id == id }) {
                WorkThreadTaskDetail(operation: task, workspace: workspace, deliveryError: library.taskDeliveryErrors[id])
                    .environment(\.returnToWorkThread, returnToThread)
            }
        }
        .navigationDestination(item: $selectedReviewID) { id in
            if let task = tasks.first(where: { $0.id == id }) {
                let display = TaskPresentation(operation: task, workspace: workspace)
                WorkThreadReviewView(operation: task, agentName: display.agent, projectName: display.project)
                    .environment(\.returnToWorkThread, returnToThread)
            }
        }
        .onAppear {
            if focusOnReturn { focusOnReturn = false; composing = true; scrollRequest &+= 1 }
        }
        .task { await load() }
        .onChange(of: library.deliveryRevision) { _, _ in Task { await load() } }
        .onChange(of: library.messageDrafts[thread.id]?.revision) { _, _ in error = nil }
    }

    private var composer: some View {
        VStack(spacing: 6) {
            if let id = followupTaskID {
                WorkThreadFollowupTarget(operation: tasks.first { $0.id == id }) {
                    library.cancelFollowup(threadID: thread.id)
                }.transition(.opacity)
            }
            if saving && followupTaskID == nil && ThreadMentions.requestsNorma(draft.wrappedValue) {
                Text("Norma 正在回复…").font(.caption).foregroundStyle(IdeaPalette.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading).accessibilityIdentifier("thread-norma-waiting")
            }
            if let error {
                Text(error).font(.caption).foregroundStyle(.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("thread-action-error")
            }
            if followupTaskID == nil && !selectedAgents.isEmpty {
                ScrollView(.horizontal) {
                    HStack(spacing: 12) {
                        ForEach(selectedAgents) { agent in
                            let project = workspace?.projects.first { $0.id == agent.projectReferenceID }
                            Text("将呼叫：\(agent.name) · \(project?.name ?? "原关联项目") · \(project?.machineName ?? "原 Mac") · \(agent.profile)")
                                .font(.caption).foregroundStyle(IdeaPalette.secondary)
                                .accessibilityIdentifier("thread-selected-agent-\(agent.id)")
                        }
                    }
                }.fixedSize(horizontal: false, vertical: true)
            }
            if let draftError = library.draftErrors[thread.id] {
                Text(draftError).font(.caption).foregroundStyle(.red)
                Button("重试保存草稿") { library.retryDraftSave(threadID: thread.id) }.frame(minHeight: 44)
            } else if !draft.wrappedValue.isEmpty {
                Text(library.savingDrafts ? "正在保存草稿…" : "草稿已保存")
                    .font(.caption).foregroundStyle(IdeaPalette.secondary)
            }
            HStack(alignment: .bottom, spacing: 12) {
            TextField(workspace?.archived == true ? "重开工作空间后可继续" : followupTaskID == nil ? "发消息或 @Agent" : "继续这个任务…", text: draft, axis: .vertical)
                .lineLimit(1...5).focused($composing).font(.body)
                .padding(.horizontal, 16).padding(.vertical, 13)
                .background(IdeaPalette.softGray, in: RoundedRectangle(cornerRadius: 23))
                .accessibilityIdentifier("thread-message-input")
            if followupTaskID == nil { Button {
                resumeComposing = composing; composing = false; showingMentions = true
            } label: {
                Image(systemName: "at").font(.system(size: 20, weight: .medium)).frame(width: 44, height: 44)
                    .background(IdeaPalette.softGray, in: Circle())
            }.accessibilityLabel("选择要呼叫的 Agent").accessibilityIdentifier("thread-mention") }
            Button { Task { await send() } } label: {
                if saving { ProgressView().frame(width: 44, height: 44) }
                else { Image(systemName: "arrow.up.circle.fill").font(.system(size: 28)).frame(width: 44, height: 44) }
            }.accessibilityLabel("发送消息").accessibilityIdentifier("thread-send")
                .disabled(saving || !loaded || draft.wrappedValue.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }.disabled(workspace?.archived != false || !loaded || !library.draftsLoaded)
        }.padding(12)
            .animation(reduceMotion ? nil : .smooth(duration: 0.25), value: followupTaskID)
            .background {
                if reduceTransparency { Color.white }
                else if #available(iOS 26.0, *) { Color.clear.glassEffect(.regular, in: RoundedRectangle(cornerRadius: 30)) }
                else { RoundedRectangle(cornerRadius: 30).fill(.regularMaterial) }
            }.padding(.horizontal, 10).padding(.bottom, 4)
    }

    private func agentReply(_ message: WorkThreadMessage) -> some View {
        let operation = tasks.first { $0.id == message.taskOperationID }
        let isNorma = message.author == "norma"
        let truncated = message.replyToMessageID == nil && operation?.receipt?.execution?.execution.result?.truncated == true
        let name = isNorma ? "Norma" : operation?.target?.name ?? "Agent"
        return HStack(alignment: .top, spacing: 12) {
            if isNorma {
                Image("NormaPortrait").resizable().scaledToFill().frame(width: 36, height: 36)
                    .clipShape(Circle()).accessibilityHidden(true)
            } else {
            Image(systemName: "chevron.left.forwardslash.chevron.right").font(.system(size: 18))
                .foregroundStyle(IdeaPalette.blue).frame(width: 36, height: 36)
                .background(IdeaPalette.blue.opacity(0.08), in: Circle()).accessibilityHidden(true)
            }
            VStack(alignment: .leading, spacing: 8) {
                Text(name).font(.subheadline.weight(.medium)).foregroundStyle(IdeaPalette.secondary)
                Text(message.text).font(.body).textSelection(.enabled).padding(14)
                    .background(.white, in: RoundedRectangle(cornerRadius: 20))
                if truncated {
                    Text("部分回复 · 完整内容保留在 Mac").font(.caption).foregroundStyle(IdeaPalette.secondary)
                }
                Text(message.createdAt, style: .time).font(.caption).foregroundStyle(IdeaPalette.secondary)
            }
        }.padding(.trailing, 20).frame(maxWidth: .infinity, alignment: .leading)
            .contextMenu {
                Button("记为共同决定", systemImage: "doc.text") { Task { await recordDecision(message) } }
                    .disabled(workspace?.archived != false || saving)
                if !isNorma { Button("查看任务", systemImage: "doc.text.magnifyingglass") { selectedTaskID = message.taskOperationID } }
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("\(name)，\(message.createdAt.formatted(date: .abbreviated, time: .shortened))，\(message.text)")
            .accessibilityValue(truncated ? "部分回复，完整内容保留在 Mac" : "")
            .accessibilityIdentifier(isNorma ? "thread-norma-message-\(message.id)" : "thread-agent-message-\(message.id)")
            .accessibilityActions {
                if !isNorma { Button("查看任务") { selectedTaskID = message.taskOperationID } }
                if workspace?.archived == false && !saving {
                    Button("记为共同决定") { Task { await recordDecision(message) } }
                }
            }
    }

    private var mentionSheet: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Text("选择成员后输入要交给它的事情，发送时一起提供当前目标与共同事实。")
                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                    Button {
                        library.mentionNorma(threadID: thread.id)
                        resumeComposing = true; showingMentions = false
                    } label: {
                        HStack(spacing: 12) {
                            Image("NormaPortrait").resizable().scaledToFill().frame(width: 44, height: 44).clipShape(Circle())
                            VStack(alignment: .leading, spacing: 4) {
                                Text("Norma").font(.headline)
                                Text("协调工作与分工").font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                            }
                            Spacer(minLength: 0)
                            Image(systemName: ThreadMentions.requestsNorma(draft.wrappedValue) ? "checkmark.circle.fill" : "plus.circle")
                                .font(.system(size: 22)).foregroundStyle(IdeaPalette.blue)
                        }.threadCard()
                    }.accessibilityIdentifier("thread-mention-norma")
                        .accessibilityValue(ThreadMentions.requestsNorma(draft.wrappedValue) ? "已选择" : "未选择")
                    if workspace?.agents.isEmpty != false {
                        ContentUnavailableView("还没有参与的 Agent", systemImage: "person.2",
                            description: Text("先在工作空间设置中关联项目，再添加参与的 Agent。"))
                    }
                    ForEach(workspace?.projects ?? []) { project in
                        let agents = workspace?.agents.filter { $0.projectReferenceID == project.id } ?? []
                        if !agents.isEmpty {
                            VStack(alignment: .leading, spacing: 12) {
                                Text(project.name + " · " + project.machineName).font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                                ForEach(agents) { agent in
                                    let selected = selectedAgents.contains { $0.id == agent.id }
                                    Button {
                                        library.mentionAgent(agent, threadID: thread.id)
                                        resumeComposing = true; showingMentions = false
                                    } label: {
                                        HStack(spacing: 12) {
                                            Image(systemName: "chevron.left.forwardslash.chevron.right")
                                                .font(.system(size: 20))
                                                .frame(width: 44, height: 44).background(IdeaPalette.softGray, in: Circle())
                                            VStack(alignment: .leading, spacing: 4) {
                                                Text(agent.name).font(.headline)
                                                Text(agent.profile).font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                                            }
                                            Spacer(minLength: 0)
                                            Image(systemName: selected ? "checkmark.circle.fill" : "plus.circle")
                                                .font(.system(size: 22)).foregroundStyle(IdeaPalette.blue)
                                        }.threadCard()
                                    }.accessibilityIdentifier("thread-mention-agent-\(agent.id)")
                                        .accessibilityValue(selected ? "已选择" : "未选择")
                                        .accessibilityAddTraits(selected ? [.isSelected] : [])
                                }
                            }
                        }
                    }
                    Button("工作空间设置", systemImage: "slider.horizontal.3") {
                        openSettingsAfterMention = true; showingMentions = false
                    }.frame(minHeight: 44).accessibilityIdentifier("thread-mention-settings")
                }.padding(20)
            }.background(IdeaPalette.background).foregroundStyle(IdeaPalette.ink)
                .navigationTitle("呼叫 Agent").navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button { showingMentions = false } label: {
                            Image(systemName: "xmark").font(.system(size: 17, weight: .semibold)).frame(width: 44, height: 44)
                        }.accessibilityLabel("关闭成员选择").accessibilityIdentifier("thread-mention-cancel")
                    }
                }
        }.tint(IdeaPalette.blue).buttonStyle(IdeaPressStyle())
            .presentationDetents([.medium, .large]).presentationDragIndicator(.visible)
    }

    private var sourceSheet: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    Text("引用时的想法").font(.title2.bold())
                    if let excerpt = thread.sourceExcerpt, !excerpt.isEmpty {
                        Text(excerpt).textSelection(.enabled)
                    } else { Text("语音想法，引用时尚未转写") }
                    Text("这是创建工作线程时保存的快照。原想法后续修改不会改写这里。")
                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                    if let current = ideas.ideas.first(where: { $0.id == thread.sourceIdeaID }) {
                        Button("打开原始想法") { originalIdea = current }
                            .frame(minHeight: 44).accessibilityIdentifier("thread-open-original-idea")
                    }
                    if let loadError = ideas.error {
                        Text(loadError).foregroundStyle(.red).font(.subheadline)
                        Button("重新读取原始想法") { Task { await ideas.load() } }.frame(minHeight: 44)
                    } else if !ideas.loaded {
                        ProgressView("正在读取原始想法…")
                    } else if !ideas.ideas.contains(where: { $0.id == thread.sourceIdeaID }) {
                        Text("原始想法暂不可用，引用快照仍保留。")
                            .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                    }
                }.padding(20)
            }.background(IdeaPalette.background)
                .navigationTitle("来源想法").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { showingSource = false } } }
                .fullScreenCover(item: $originalIdea) { idea in
                    IdeaTextEditorView(model: ideas, editing: idea) { originalIdea = nil }
                }
                .task { await ideas.load() }
        }.presentationDetents([.medium, .large]).presentationDragIndicator(.visible)
    }

    private func returnToThread(_ operationID: String?) async throws {
        if let operationID { try await library.beginFollowup(operationID: operationID, threadID: thread.id) }
        focusOnReturn = operationID != nil
        selectedTaskID = nil; selectedReviewID = nil
    }

    private func load() async {
        let request = UUID(); messageRead = request
        do {
            let latest = try await library.store.messages(threadID: thread.id)
            let latestTasks = try await library.store.threadTasks(threadID: thread.id)
            let latestFollowups = try await library.store.threadFollowups(threadID: thread.id)
            guard messageRead == request else { return }
            messages = latest; tasks = latestTasks; followups = latestFollowups; loaded = true; loadError = nil
        } catch { if messageRead == request { loadError = error.localizedDescription } }
    }
    private func send() async {
        guard !saving else { return }
        saving = true
        defer { saving = false }
        let text = draft.wrappedValue
        do {
            let message = try await library.sendMessage(threadID: thread.id, text: text, cloud: cloud)
            messageRead = UUID() // An older refresh cannot replace a message that just finished saving.
            if !messages.contains(where: { $0.id == message.id }) { messages.append(message) }
            composing = false; error = nil
            await load()
            scrollRequest &+= 1
        } catch { self.error = error.localizedDescription }
    }
    private func recordDecision(_ message: WorkThreadMessage) async {
        guard !saving else { return }
        saving = true
        defer { saving = false }
        do {
            _ = try await library.store.saveFact(threadID: thread.id, kind: .decision, text: message.text, sourceMessageID: message.id)
            error = nil; showingBrief = true
        } catch { self.error = error.localizedDescription }
    }
}

extension WorkThreadMessage {
    func timelineCards(tasks: [ThreadTaskOutboxOperation], history: [WorkThreadMessage])
        -> (tasks: [ThreadTaskOutboxOperation], deliveries: [ThreadTaskOutboxOperation]) {
        let source = author == "norma" ? replyToMessageID : history.contains { $0.author == "norma" && $0.replyToMessageID == id } ? nil : id
        let attached = tasks.filter { $0.task.sourceMessageID == source }
        let deliveries = tasks.filter { $0.replyMessageID == id || ($0.replyMessageID == nil && attached.contains($0)) }
        return (attached, deliveries)
    }
}

extension View {
    func threadCard() -> some View {
        padding(16).frame(maxWidth: .infinity, alignment: .leading)
            .background(.white, in: RoundedRectangle(cornerRadius: 22))
    }
}
