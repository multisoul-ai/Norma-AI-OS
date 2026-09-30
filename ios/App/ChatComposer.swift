import SwiftUI
import NormaCore

struct ChatComposer: View {
    let model: ComputerModel
    let machine: PairedMachine
    let workspaceID: String
    let routeID: String
    let project: ProjectSnapshot?
    let card: CardSnapshot
    let revision: UInt64
    let canControl: Bool
    var taskForm = false
    var saved: () -> Void = {}
    let accepted: (MobileCommandReceipt.CommandResult) -> Void
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.dynamicTypeSize) private var typeSize
    @FocusState private var inputFocused: Bool
    @State private var busy = false
    @State private var status: String?
    @State private var work: Task<Void, Never>?
    @State private var showResult = false
    @State private var showDrafts = false
    @State private var showQueue = false
    @State private var loadingImages = false
    @State private var imageError: String?
    @State private var uploadProgress: Double?
    @State private var recording = false
    private var scope: ChatDraft.Scope {
        ChatDraft.Scope(machineID: machine.id, workspaceID: workspaceID, routeID: routeID, nodeID: card.id, runID: card.runID, sessionID: card.sessionID)
    }
    private var text: String { model.drafts.draft(for: scope)?.text ?? "" }
    private var images: [ChatImage] { model.drafts.draft(for: scope)?.imageAttachments ?? [] }
    private var draftLabel: String { [machine.name, project?.displayName, card.title].compactMap { $0 }.joined(separator: " · ") }
    private var oldDrafts: [ChatDraft] {
        model.drafts.drafts.filter { $0.scope.machineID == machine.id && $0.scope.workspaceID == workspaceID && $0.scope.nodeID == card.id && $0.scope != scope }
    }
    private var ready: Bool { ["idle", "ready", "done", "ended", "failed"].contains(card.state) }
    private var pending: PendingMobileCommand? {
        model.pendingCommands.first {
            guard $0.machineID == machine.id, $0.request.workspaceID == workspaceID,
                  $0.request.routeID == routeID, let (expected, _) = $0.request.command.messageContent,
                  !$0.request.command.isQueuedMessage || !model.acknowledgedQueueIDs.contains($0.id) else { return false }
            return expected.nodeID == card.id
        }
    }
    private var queued: [PendingMobileCommand] {
        model.pendingCommands.filter {
            $0.machineID == machine.id && $0.request.workspaceID == workspaceID && $0.request.routeID == routeID
            && $0.request.command.isQueuedMessage && $0.request.command.messageContent?.card.id == card.id
        }
    }
    private var queueable: Bool { card.runID != nil && card.sessionID != nil && !["unknown", "preparing"].contains(card.state) }
    private var useQueue: Bool {
        queueable && (!ready || queued.contains { $0.request.command.messageContent?.card.runID == card.runID && model.queueStatuses[$0.id]?.isTerminal != true })
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !queued.isEmpty {
                Button { showQueue = true } label: {
                    Label("消息队列（\(queued.count)）", systemImage: "text.line.first.and.arrowtriangle.forward")
                        .font(.caption).frame(minHeight: 44)
                }.accessibilityIdentifier("chat-message-queue")
            }
            if let pending, !pending.request.command.isQueuedMessage {
                Button { showResult = true } label: {
                    Label(busy ? "正在确认发送…" : "查看这条消息的发送结果", systemImage: "clock")
                        .font(.caption).frame(minHeight: 44)
                }.disabled(busy)
                .task(id: pending.id) { if !busy && scenePhase == .active { await check(pending) } }
            } else if let status { Text(status).font(.caption).foregroundStyle(.secondary) }
            if !images.isEmpty {
                ScrollView(.horizontal) {
                    HStack(spacing: 8) {
                        ForEach(Array(images.enumerated()), id: \.element.id) { index, image in
                            ChatPhotoTile(id: image.id, label: "图片 \(index + 1)", load: { try await model.drafts.imageData(image) },
                                remove: busy ? nil : { model.drafts.removeImage(image.id, for: scope) })
                        }
                    }.padding(.vertical, 4)
                }.scrollIndicators(.hidden).accessibilityIdentifier("chat-photo-attachments")
            }
            if loadingImages { WorkspaceLoadingView(message: "正在准备图片…", inline: true) }
            if let uploadProgress { ProgressView("正在上传图片…", value: uploadProgress).accessibilityIdentifier("chat-image-upload-progress") }
            if let imageError { Text(imageError).foregroundStyle(.orange) }
            if taskForm {
                Text("你希望完成什么？").foregroundStyle(WorkspaceStyle.muted).font(.caption)
                photoPicker
                messageInput
                Label("发送到 " + machine.name, systemImage: "laptopcomputer")
                    .fixedSize(horizontal: false, vertical: true).padding(.vertical, 6)
                (typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(spacing: 10)) : AnyLayout(HStackLayout(spacing: 10))) {
                    Button { work = Task { await keepDraft() } } label: {
                        Text("保留草稿").font(.footnote.weight(.semibold)).foregroundStyle(.white)
                            .frame(maxWidth: .infinity, minHeight: 48)
                            .background(WorkspaceStyle.raised, in: RoundedRectangle(cornerRadius: 16))
                            .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                    }.disabled(busy || loadingImages || !model.drafts.loaded || (text.isEmpty && images.isEmpty)).accessibilityIdentifier("new-task-save")
                    sendButton
                }.padding(.top, 4).buttonStyle(WorkspacePressStyle())
            } else {
                HStack(alignment: .bottom, spacing: 8) {
                    ChatVoiceInput(
                        text: Binding(get: { text }, set: { model.drafts.update($0, for: scope, label: draftLabel) }),
                        recording: $recording, disabled: busy || !model.drafts.loaded,
                        focus: { inputFocused = $0 }
                    ) { messageInput }
                    .id(scope)
                    photoPicker
                    sendButton
                }
            }
            if !canControl { Text("无法发送，请检查连接与权限。") }
            else if card.state == "collecting" { Text("正在保存本轮成果，下一步会排队等待。") }
            else if useQueue { Text("加入队列，Agent 空闲后依次处理。") }
            else if text.utf8.count > 32 * 1024 { Text("消息超过长度上限，请分成几条发送。") }
            if card.profile == "Claude" && text.contains("\t") { Text("Claude 会将 Tab 转为 4 个空格。") }
            if let failure = model.drafts.failure {
                Text(failure).foregroundStyle(.orange).accessibilityIdentifier("draft-storage-error")
                Button("重试保存草稿") { Task { await model.drafts.retry() } }.frame(minHeight: 44)
            }
            if !oldDrafts.isEmpty {
                Button("历史草稿（\(oldDrafts.count)）") { showDrafts = true }.frame(minHeight: 44)
            }
        }
        .font(.caption).foregroundStyle(.secondary).padding(.horizontal, taskForm ? 0 : 12).padding(.vertical, taskForm ? 0 : 8).background(.black)
        .toolbar {
            // iOS 26's keyboard accessory can intercept composer taps after restoring focus.
            if inputFocused {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("收起键盘", systemImage: "keyboard.chevron.compact.down") { inputFocused = false }
                        .labelStyle(.iconOnly).frame(minWidth: 44, minHeight: 44).accessibilityIdentifier("chat-dismiss-keyboard")
                }
            }
        }
        .sheet(isPresented: $showResult) {
            if let project, let pending { CardCommandSheet(model: model, machine: machine, project: project, revision: revision, pending: pending) }
        }
        .sheet(isPresented: $showQueue) { MessageQueueView(model: model, commands: queued) }
        .sheet(isPresented: $showDrafts) { DraftHistoryView(book: model.drafts, drafts: oldDrafts) }
        .onChange(of: scenePhase) { _, phase in if phase != .active { work?.cancel() } }
        .onDisappear { work?.cancel() }
    }

    private var photoPicker: some View {
        ChatPhotoPicker(book: model.drafts, scope: scope, label: draftLabel, disabled: busy || recording || pending != nil || !model.drafts.loaded,
                        loading: $loadingImages, error: $imageError)
    }

    private var messageInput: some View {
        TextField(taskForm ? "例如：修复语音输入重复转写，并补充回归测试" : "告诉 Agent 你的想法…", text: Binding(get: { text }, set: { model.drafts.update($0, for: scope, label: draftLabel) }), axis: .vertical)
            .focused($inputFocused).font(taskForm ? .footnote : .subheadline).foregroundStyle(.primary)
            // Keep room for the conversation above the keyboard at accessibility sizes; text stays scrollable.
            .lineLimit(taskForm ? 4...8 : 1...(typeSize.isAccessibilitySize && inputFocused ? 1 : typeSize.isAccessibilitySize ? 2 : 5))
            .frame(minHeight: taskForm ? 120 : nil, alignment: .topLeading)
            .padding(.horizontal, 14).padding(.vertical, taskForm ? 12 : 10)
            .frame(minHeight: taskForm ? nil : 44)
            .background(WorkspaceStyle.surface, in: RoundedRectangle(cornerRadius: 22))
            .overlay(RoundedRectangle(cornerRadius: 22).strokeBorder(inputFocused ? WorkspaceStyle.muted : WorkspaceStyle.line, lineWidth: 1))
            .disabled(busy || !model.drafts.loaded).accessibilityIdentifier(taskForm ? "new-task-input" : "chat-message-input")
    }

    private var sendButton: some View {
        Button { inputFocused = false; work = Task { await send() } } label: {
            if taskForm {
                Label(useQueue ? "加入队列" : "发送到 Mac", systemImage: "arrow.up")
                    .font(.footnote.weight(.semibold)).foregroundStyle(.black)
                    .frame(maxWidth: .infinity, minHeight: 48)
                    .background(.white, in: RoundedRectangle(cornerRadius: 16))
            } else {
                Image(systemName: useQueue ? "text.line.first.and.arrowtriangle.forward" : "arrow.up").font(.system(size: 17, weight: .semibold))
                    .frame(width: 44, height: 44).background(.white, in: Circle()).foregroundStyle(.black)
            }
        }
        .disabled(!canControl || !card.mobileControl || !(ready || queueable) || busy || recording || loadingImages || !model.drafts.loaded || model.drafts.failure != nil || pending != nil || (text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && images.isEmpty) || text.utf8.count > 32 * 1024)
        .opacity(canControl && card.mobileControl && (ready || queueable) && pending == nil && !busy ? 1 : 0.35)
        .accessibilityLabel(useQueue ? "加入队列" : taskForm ? "发送到 Mac" : "发送消息")
        .accessibilityIdentifier(taskForm ? "new-task-send" : "chat-send-message")
    }

    @MainActor private func keepDraft() async {
        guard !busy else { return }
        busy = true; inputFocused = false
        defer { busy = false }
        do { try await model.drafts.flush(); try Task.checkCancellation(); saved() }
        catch { status = error.localizedDescription }
    }
    @MainActor private func send() async {
        guard !busy, !recording, pending == nil, let project, let route = project.routeID,
              let draft = model.drafts.draft(for: scope) else { return }
        busy = true; status = nil
        defer { busy = false; uploadProgress = nil }
        do {
            try await model.drafts.flush()
            var paths: [String] = []
            for (index, image) in draft.imageAttachments.enumerated() {
                let data = try await model.drafts.imageData(image)
                let uploaded = try await model.client.uploadImage(data, to: machine, workspaceID: project.id, revision: revision) { progress in
                    await MainActor.run { uploadProgress = (Double(index) + progress) / Double(draft.imageAttachments.count) }
                }
                paths.append(uploaded.path)
            }
            try Task.checkCancellation()
            let message = paths.isEmpty ? draft.text : try ChatImageMessage.compose(text: draft.text, paths: paths)
            let command = try await useQueue
                ? model.client.prepareQueuedMessage(machine, workspaceID: project.id, routeID: route, revision: revision, card: card, text: message, draftID: draft.id)
                : model.client.prepareMessage(machine, workspaceID: project.id, routeID: route, revision: revision, card: card, text: message, draftID: draft.id)
            try await model.commandStore.save(command)
            try await model.reloadCommands()
            try Task.checkCancellation()
            let receipt = try await model.client.submit(command, to: machine)
            if command.request.command.isQueuedMessage {
                try await model.confirmQueue(command, receipt: receipt)
                status = receipt.state == "succeeded" ? "已加入队列" : "入队尚未确认，请查看消息队列。"
                if taskForm, receipt.state == "succeeded", let result = receipt.result {
                    try Task.checkCancellation(); accepted(result)
                }
            } else { try await finish(command) }
        } catch { status = error.localizedDescription }
    }
    @MainActor private func check(_ command: PendingMobileCommand) async {
        guard !busy else { return }
        busy = true
        defer { busy = false }
        do { try await finish(command) } catch { status = error.localizedDescription }
    }
    @MainActor private func finish(_ command: PendingMobileCommand) async throws {
        // 50 × 500ms = 25s bounded foreground receipt polling. Reconnect never resubmits.
        for _ in 0..<50 {
            let receipt = try await model.client.receipt(for: command, on: machine)
            try Task.checkCancellation()
            if receipt.state == "succeeded", let result = receipt.result {
                try await model.drafts.confirm(command, receipt: receipt)
                try await model.commandStore.remove(id: command.id)
                try await model.reloadCommands()
                status = "已发送"; accepted(result)
                return
            }
            if receipt.state == "rejected" {
                // A verified rejection executed nothing: retain the draft, release only this operation.
                // Unknown outcomes remain pending and must never be retried automatically.
                try await model.commandStore.remove(id: command.id)
                try await model.reloadCommands()
                status = receipt.rejectionMessage(actionName: "发送")
                return
            }
            if receipt.state != "accepted" { status = "发送未确认，请查看结果。"; return }
            try await Task.sleep(for: .milliseconds(500))
        }
        status = "等待电脑确认，可以稍后查询结果。"
    }
}
