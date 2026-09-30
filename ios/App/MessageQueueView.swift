import SwiftUI
import NormaCore

struct MessageQueueView: View {
    let model: ComputerModel
    let commands: [PendingMobileCommand]
    @Environment(\.dismiss) private var dismiss
    private var records: [PendingMobileCommand] { commands.filter { command in model.pendingCommands.contains { $0.id == command.id } } }
    var body: some View {
        NavigationStack {
            List {
                if records.isEmpty { ContentUnavailableView("没有排队消息", systemImage: "text.line.first.and.arrowtriangle.forward") }
                ForEach(records) { command in
                    NavigationLink {
                        QueuedMessageDetail(model: model, command: command)
                    } label: {
                        VStack(alignment: .leading, spacing: 6) {
                            let message = command.request.command.messageContent?.text ?? ""
                            let images = ChatImageMessage.parse(message)
                            Text(images?.text.isEmpty == true ? "图片消息" : images?.text ?? message).font(.subheadline).lineLimit(2)
                            if let images { Label("\(images.paths.count) 张图片", systemImage: "photo").font(.caption).foregroundStyle(.secondary) }
                            Text([model.machines.first { $0.id == command.machineID }?.name ?? "已移除的电脑", queueProjectName(model, command), command.request.command.displayName].joined(separator: " · "))
                                .font(.caption).foregroundStyle(.secondary)
                            Text("上次状态：" + queueLabel(model.queueStatuses[command.id])).font(.caption).foregroundStyle(.secondary)
                        }.padding(.vertical, 6)
                    }
                }
            }
            .scrollContentBackground(.hidden).background(.black)
            .navigationTitle("消息队列").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("关闭") { dismiss() }.accessibilityIdentifier("queue-close") } }
        }
    }
}

@MainActor private func queueProjectName(_ model: ComputerModel, _ command: PendingMobileCommand) -> String {
    if model.selectedID == command.machineID, let project = model.projects?.first(where: { $0.id == command.request.workspaceID }) { return project.displayName }
    return command.request.workspaceID
}

private func queueLabel(_ status: QueuedMessageStatus?) -> String {
    switch status?.state {
    case "queued": "等待 Agent 空闲"
    case "dispatching": "正在发送，暂不可撤回"
    case "sent": "Agent 已收到"
    case "cancelled": "已取消，原文已保留"
    case "outcome_unknown": "发送结果未知，请核对电脑"
    default: "等待查询电脑上的状态"
    }
}

private struct QueuedMessageDetail: View {
    let model: ComputerModel
    let command: PendingMobileCommand
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var scenePhase
    @State private var busy = false
    @State private var error: String?
    @State private var checkedAt: Date?
    @State private var clearing = false
    @State private var copied = false
    private var machine: PairedMachine? { model.machines.first { $0.id == command.machineID } }
    private var status: QueuedMessageStatus? { model.queueStatuses[command.id] }
    var body: some View {
        Form {
            Section {
                LabeledContent("电脑", value: machine?.name ?? "已移除的电脑")
                LabeledContent("项目", value: queueProjectName(model, command))
                LabeledContent("卡片", value: command.request.command.displayName)
                Text((checkedAt == nil && status != nil ? "上次状态：" : "") + queueLabel(status)).fontWeight(.medium).accessibilityIdentifier("queued-message-status")
                if let checkedAt { Text("核验于 \(checkedAt.formatted(date: .omitted, time: .standard))").font(.caption).foregroundStyle(.secondary) }
                if let error { Text(error).font(.caption).foregroundStyle(.orange) }
                if busy { WorkspaceLoadingView(message: "正在核验…", inline: true) }
                Button("查询最新状态") { Task { await refresh() } }.disabled(busy || machine == nil)
                if status?.state == "queued" {
                    Button("撤回这条消息", role: .destructive) { Task { await cancel() } }
                        .disabled(busy || machine == nil).accessibilityIdentifier("cancel-queued-message")
                }
            } footer: { Text("重新连接只查询状态。只有尚未开始发送的消息可以撤回；入队成功不代表 Agent 已收到。") }
            Section("消息原文") {
                let message = command.request.command.messageContent?.text ?? ""
                Text(ChatImageMessage.parse(message)?.text ?? message).textSelection(.enabled)
                if let images = ChatImageMessage.parse(message), let machine {
                    ScrollView(.horizontal) {
                        HStack(spacing: 8) {
                            ForEach(Array(images.paths.enumerated()), id: \.offset) { index, path in
                                if let id = ChatImageMessage.imageID(path: path) {
                                    ChatPhotoTile(id: id, label: "图片 \(index + 1)", load: {
                                        try await model.client.chatImage(id, on: machine, workspaceID: command.request.workspaceID)
                                    })
                                }
                            }
                        }
                    }.scrollIndicators(.hidden)
                }
                Button(copied ? "已复制原文" : "复制原文") { UIPasteboard.general.string = command.request.command.messageContent?.text; copied = true }
                    .accessibilityIdentifier("copy-queued-message")
            }
            if status?.isTerminal == true || (error != nil && status == nil) || machine == nil {
                Section {
                    Button("清除本机记录", role: .destructive) { clearing = true }.disabled(busy)
                } footer: { Text("原文会一直保留，直到你主动清除。清除本机记录不会停止电脑上的任务。") }
            }
        }
        .font(.subheadline).scrollContentBackground(.hidden).background(.black)
        .navigationTitle("排队消息").navigationBarTitleDisplayMode(.inline)
        .confirmationDialog("清除这条消息的本机记录？", isPresented: $clearing, titleVisibility: .visible) {
            Button("已核对，清除记录", role: .destructive) {
                Task {
                    do {
                        try await model.commandStore.remove(id: command.id); try await model.reloadCommands()
                        model.queueStatuses.removeValue(forKey: command.id); model.acknowledgedQueueIDs.remove(command.id)
                        dismiss()
                    } catch { self.error = error.localizedDescription }
                }
            }
        } message: { Text("请先复制需要保留的原文。结果未知时，先在电脑核对，避免重复安排任务。") }
        .task(id: scenePhase) {
            guard scenePhase == .active else { return }
            repeat {
                await refresh()
                if error != nil || status?.isTerminal == true { return }
                do { try await Task.sleep(for: .seconds(2)) } catch { return }
            } while !Task.isCancelled
        }
    }
    @MainActor private func refresh() async {
        guard !busy, let machine else { return }
        busy = true; error = nil
        defer { busy = false }
        do { try await model.refreshQueue(command, machine: machine); checkedAt = .now }
        catch { self.error = error.localizedDescription }
    }
    @MainActor private func cancel() async {
        guard !busy, let machine else { return }
        busy = true; error = nil
        defer { busy = false }
        do {
            let grant = try await model.client.authenticate(machine)
            model.queueStatuses[command.id] = try await model.client.cancelQueuedMessage(command, on: machine, revision: grant.revision)
            checkedAt = .now
        } catch { self.error = error.localizedDescription }
    }
}
